use crate::ansi::AnsiString;
use crate::event::{Event, EventSender};
use nix::libc;

use crate::field::get_string_by_range;
use crate::item::Item;
use crate::spinlock::SpinLock;
use crate::util::escape_single_quote;
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::env;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use tuikit::prelude::*;
use std::thread::JoinHandle;

const TAB_STOP: usize = 8;
const DELIMITER_STR: &str = r"[\t\n ]+";

lazy_static! {
    static ref RE_FIELDS: Regex = Regex::new(r"\\?(\{-?[0-9.,q]*?})").unwrap();
}

pub struct Previewer {
    tx_preview: Sender<(Event, PreviewInput)>,
    content: Arc<SpinLock<AnsiString>>,
    width: AtomicUsize,
    height: AtomicUsize,
    prev_item: Option<Arc<Item>>,
    preview_cmd: Option<String>,
    delimiter: Regex,
    wrap: bool,
    thread_previewer: Option<JoinHandle<()>>,
}

impl Previewer {
    pub fn new(preview_cmd: Option<String>) -> Self {
        let content = Arc::new(SpinLock::new(AnsiString::new_empty()));
        let (tx_preview, rx_preview) = channel();
        let content_clone = content.clone();
        let thread_previewer = thread::spawn(move || run(rx_preview, content_clone));

        Self {
            tx_preview,
            content,
            width: AtomicUsize::new(80),
            height: AtomicUsize::new(60),
            prev_item: None,
            preview_cmd,
            delimiter: Regex::new(DELIMITER_STR).unwrap(),
            wrap: false,
            thread_previewer: Some(thread_previewer),
        }
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn delimiter(mut self, delimiter: Regex) -> Self {
        self.delimiter = delimiter;
        self
    }

    pub fn on_item_change(&mut self, item: Arc<Item>) {
        if self
            .prev_item
            .as_ref()
            .map(|prev| prev.get_output_text() == item.get_output_text())
            .unwrap_or(false)
        {
            return;
        }

        self.prev_item.replace(item);
        let text = self.prev_item.as_ref().map(|item| item.get_output_text()).unwrap();
        let cmd = self.inject_preview_command(&text).to_string();
        let columns = self.width.load(Ordering::Relaxed);
        let lines = self.height.load(Ordering::Relaxed);

        let request = PreviewInput { cmd, columns, lines };
        let _ = self.tx_preview.send((Event::EvPreviewRequest, request));
    }

    fn inject_preview_command(&self, text: &str) -> Cow<str> {
        let cmd = self
            .preview_cmd
            .as_ref()
            .expect("model:inject_preview_command: invalid preview command");
        RE_FIELDS.replace_all(cmd, |caps: &Captures| {
            // \{...
            if &caps[0][0..1] == "\\" {
                return caps[0].to_string();
            }

            // {1..} and other variant
            assert!(caps[1].len() >= 2);
            let range = &caps[1][1..caps[1].len() - 1];
            let replacement = if range == "" {
                text
            } else {
                get_string_by_range(&self.delimiter, text, range).unwrap_or("")
            };

            format!("'{}'", escape_single_quote(replacement))
        })
    }
}

impl Drop for Previewer {
    fn drop(&mut self) {
        let request = PreviewInput {
            cmd: "".to_string(),
            columns: 0,
            lines: 0,
        };
        let _ = self.tx_preview.send((Event::EvActAbort, request));
        self.thread_previewer.take().map(|handle| handle.join());
    }
}

impl Draw for Previewer {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        canvas.clear()?;
        let (screen_width, screen_height) = canvas.size()?;

        if screen_width == 0 || screen_height == 0 {
            return Ok(());
        }

        self.width.store(screen_width, Ordering::Relaxed);
        self.height.store(screen_height, Ordering::Relaxed);

        let content = self.content.lock();

        let mut row = 0;
        let mut col = 0;
        for (ch, attr) in content.iter() {
            match ch {
                '\r' | '\0' => {}
                '\n' => {
                    row += 1;
                    col = 0;
                }
                '\t' => {
                    // handle tabstop
                    let rest = TAB_STOP - col % TAB_STOP;
                    for _ in 0..rest {
                        col += canvas.put_char_with_attr(row, col, ' ', attr)?;
                    }
                }

                ch => {
                    col += canvas.put_char_with_attr(row, col, ch, attr)?;
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct PreviewInput {
    pub cmd: String,
    pub lines: usize,
    pub columns: usize,
}

struct PreviewThread {
    pid: u32,
    thread: thread::JoinHandle<()>,
    stopped: Arc<AtomicBool>,
}

impl PreviewThread {
    fn kill(self) {
        if !self.stopped.load(Ordering::Relaxed) {
            unsafe { libc::kill(self.pid as i32, libc::SIGKILL) };
        }
        self.thread.join().expect("Failed to join Preview process");
    }
}

pub fn run(rx_preview: Receiver<(Event, PreviewInput)>, content: Arc<SpinLock<AnsiString>>) {
    let mut preview_thread: Option<PreviewThread> = None;
    while let Ok((_ev, mut new_prv)) = rx_preview.recv() {
        if preview_thread.is_some() {
            preview_thread.unwrap().kill();
            preview_thread = None;
        }

        if _ev == Event::EvActAbort {
            return;
        }

        // Try to empty the channel. Happens when spamming up/down or typing fast.
        while let Ok((_ev, new_prv1)) = rx_preview.try_recv() {
            if _ev == Event::EvActAbort {
                return;
            }
            new_prv = new_prv1;
        }

        let cmd = &new_prv.cmd;
        if cmd == "" {
            continue;
        }

        let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
        let spawned = Command::new(shell)
            .env("LINES", new_prv.lines.to_string())
            .env("COLUMNS", new_prv.columns.to_string())
            .arg("-c")
            .arg(&cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        match spawned {
            Err(err) => {
                let astdout = AnsiString::from_str(format!("Failed to spawn: {} / {}", cmd, err).as_str());
                *content.lock() = astdout;
                preview_thread = None;
            }
            Ok(spawned) => {
                let pid = spawned.id();
                let stopped = Arc::new(AtomicBool::new(false));
                let stopped_clone = stopped.clone();
                let content_clone = content.clone();
                let thread = thread::spawn(move || wait_and_update(spawned, content_clone, stopped_clone));
                preview_thread = Some(PreviewThread { pid, thread, stopped });
            }
        }
    }
}

fn wait_and_update(mut spawned: std::process::Child, content: Arc<SpinLock<AnsiString>>, stopped: Arc<AtomicBool>) {
    let status = spawned.wait();
    stopped.store(true, Ordering::SeqCst);

    if status.is_err() {
        return;
    }
    let status = status.unwrap();

    // Capture stderr in case users want to debug ...
    let mut pipe: Box<Read> = if status.success() {
        Box::new(spawned.stdout.unwrap())
    } else {
        Box::new(spawned.stderr.unwrap())
    };
    let mut res: Vec<u8> = Vec::new();
    pipe.read_to_end(&mut res).expect("Failed to read from std pipe");
    let stdout = String::from_utf8_lossy(&res).to_string();
    let astdout = AnsiString::from_str(&stdout);
    *content.lock() = astdout;
}

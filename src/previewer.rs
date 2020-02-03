use crate::ansi::AnsiString;
use crate::event::{Event, EventHandler, UpdateScreen};
use crate::item::Item;
use crate::spinlock::SpinLock;
use crate::util::{inject_command, InjectContext};
use derive_builder::Builder;
use nix::libc;
use regex::Regex;
use std::cmp::{max, min};
use std::env;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use tuikit::prelude::{Event as TermEvent, *};

const TAB_STOP: usize = 8;
const DELIMITER_STR: &str = r"[\t\n ]+";

pub struct Previewer {
    tx_preview: Sender<PreviewEvent>,
    content_lines: Arc<SpinLock<Vec<AnsiString>>>,

    width: AtomicUsize,
    height: AtomicUsize,
    hscroll_offset: usize,
    vscroll_offset: usize,
    wrap: bool,

    prev_item: Option<Arc<Item>>,
    prev_query: Option<String>,
    prev_cmd_query: Option<String>,
    prev_num_selected: usize,

    preview_cmd: Option<String>,
    delimiter: Regex,
    thread_previewer: Option<JoinHandle<()>>,
}

impl Previewer {
    pub fn new<C>(preview_cmd: Option<String>, callback: C) -> Self
    where
        C: Fn() + Send + Sync + 'static,
    {
        let content_lines = Arc::new(SpinLock::new(Vec::new()));
        let (tx_preview, rx_preview) = channel();
        let content_clone = content_lines.clone();
        let thread_previewer = thread::spawn(move || {
            run(rx_preview, move |lines| {
                *content_clone.lock() = lines;
                callback();
            })
        });

        Self {
            tx_preview,
            content_lines,

            width: AtomicUsize::new(80),
            height: AtomicUsize::new(60),
            hscroll_offset: 0,
            vscroll_offset: 0,
            wrap: false,

            prev_item: None,
            prev_query: None,
            prev_cmd_query: None,
            prev_num_selected: 0,

            preview_cmd,
            delimiter: Regex::new(DELIMITER_STR).unwrap(),
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

    pub fn on_item_change(
        &mut self,
        new_item: impl Into<Option<Arc<Item>>>,
        new_query: impl Into<Option<String>>,
        new_cmd_query: impl Into<Option<String>>,
        num_selected: usize,
        get_selected_items: impl Fn() -> Vec<Arc<Item>>, // lazy get
    ) {
        let new_item = new_item.into();
        let new_query = new_query.into();
        let new_cmd_query = new_cmd_query.into();

        let item_changed = match (self.prev_item.as_ref(), new_item.as_ref()) {
            (None, None) => false,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (Some(prev), Some(cur)) => prev.get_output_text() != cur.get_output_text(),
        };

        let query_changed = match (self.prev_query.as_ref(), new_query.as_ref()) {
            (None, None) => false,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (Some(prev), Some(cur)) => prev != cur,
        };

        let cmd_query_changed = match (self.prev_cmd_query.as_ref(), new_cmd_query.as_ref()) {
            (None, None) => false,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (Some(prev), Some(cur)) => prev != cur,
        };

        let selected_items_changed = self.prev_num_selected != num_selected;

        if !item_changed && !query_changed && !cmd_query_changed && !selected_items_changed {
            return;
        }

        self.prev_item = new_item;
        self.prev_query = new_query;
        self.prev_cmd_query = new_cmd_query;
        self.prev_num_selected = num_selected;

        let cmd = self.preview_cmd.as_ref().expect("previewer: invalid preview command");
        let current_selection = self
            .prev_item
            .as_ref()
            .map(|item| item.get_output_text())
            .unwrap_or_else(|| "".into());
        let query = self.prev_query.as_ref().map(|s| &**s).unwrap_or("");
        let cmd_query = self.prev_cmd_query.as_ref().map(|s| &**s).unwrap_or("");
        let selected_items = get_selected_items();
        let selected_texts: Vec<&str> = selected_items.iter().map(|item| item.get_text()).collect();

        let context = InjectContext {
            delimiter: &self.delimiter,
            current_selection: &current_selection,
            selections: &selected_texts,
            query: &query,
            cmd_query: &cmd_query,
        };

        let cmd = inject_command(cmd, context).to_string();

        let columns = self.width.load(Ordering::Relaxed);
        let lines = self.height.load(Ordering::Relaxed);
        let request = PreviewInput { cmd, columns, lines };
        let _ = self.tx_preview.send(PreviewEvent::EvPreviewRequest(request));

        self.hscroll_offset = 0;
        self.vscroll_offset = 0;
    }

    fn act_scroll_down(&mut self, diff: i32) {
        if diff > 0 {
            self.vscroll_offset += diff as usize;
        } else {
            self.vscroll_offset -= min((-diff) as usize, self.vscroll_offset);
        }

        self.vscroll_offset = min(self.vscroll_offset, max(self.content_lines.lock().len(), 1) - 1);
    }

    fn act_scroll_right(&mut self, diff: i32) {
        if diff > 0 {
            self.hscroll_offset += diff as usize;
        } else {
            self.hscroll_offset -= min((-diff) as usize, self.hscroll_offset);
        }
    }

    fn act_toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
    }
}

impl Drop for Previewer {
    fn drop(&mut self) {
        let _ = self.tx_preview.send(PreviewEvent::EvAbort);
        self.thread_previewer.take().map(|handle| handle.join());
    }
}

impl EventHandler for Previewer {
    fn handle(&mut self, event: &Event) -> UpdateScreen {
        use crate::event::Event::*;
        let height = self.height.load(Ordering::Relaxed);
        match event {
            EvActTogglePreviewWrap => self.act_toggle_wrap(),
            EvActPreviewUp(diff) => self.act_scroll_down(-*diff),
            EvActPreviewDown(diff) => self.act_scroll_down(*diff),
            EvActPreviewLeft(diff) => self.act_scroll_right(-*diff),
            EvActPreviewRight(diff) => self.act_scroll_right(*diff),
            EvActPreviewPageUp(diff) => self.act_scroll_down(-(height as i32 * *diff)),
            EvActPreviewPageDown(diff) => self.act_scroll_down(height as i32 * *diff),
            _ => return UpdateScreen::DONT_REDRAW,
        }
        UpdateScreen::REDRAW
    }
}

impl Draw for Previewer {
    fn draw(&self, canvas: &mut dyn Canvas) -> Result<()> {
        canvas.clear()?;
        let (screen_width, screen_height) = canvas.size()?;

        if screen_width == 0 || screen_height == 0 {
            return Ok(());
        }

        self.width.store(screen_width, Ordering::Relaxed);
        self.height.store(screen_height, Ordering::Relaxed);

        let content = self.content_lines.lock();

        let mut printer = PrinterBuilder::default()
            .width(screen_width)
            .height(screen_height)
            .skip_rows(self.vscroll_offset)
            .skip_cols(self.hscroll_offset)
            .wrap(self.wrap)
            .build()
            .unwrap();
        printer.print_lines(canvas, &content);

        // print the vscroll info
        let status = format!("{}/{}", self.vscroll_offset + 1, content.len());
        let col = max(status.len() + 1, self.width.load(Ordering::Relaxed)) - status.len() - 1;
        canvas.print_with_attr(
            0,
            col,
            &status,
            Attr {
                effect: Effect::REVERSE,
                ..Attr::default()
            },
        )?;

        Ok(())
    }
}

impl Widget<Event> for Previewer {
    fn on_event(&self, event: TermEvent, _rect: Rectangle) -> Vec<Event> {
        let mut ret = vec![];
        match event {
            TermEvent::Key(Key::MousePress(MouseButton::WheelUp, ..)) => ret.push(Event::EvActPreviewUp(1)),
            TermEvent::Key(Key::MousePress(MouseButton::WheelDown, ..)) => ret.push(Event::EvActPreviewDown(1)),
            _ => {}
        }
        ret
    }
}

#[derive(Debug, Ord, PartialOrd, PartialEq, Eq)]
pub struct PreviewInput {
    pub cmd: String,
    pub lines: usize,
    pub columns: usize,
}

#[derive(Debug, Ord, PartialOrd, PartialEq, Eq)]
enum PreviewEvent {
    EvPreviewRequest(PreviewInput),
    EvAbort,
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

fn run<C>(rx_preview: Receiver<PreviewEvent>, on_return: C)
where
    C: Fn(Vec<AnsiString>) + Send + Sync + 'static,
{
    let callback = Arc::new(on_return);
    let mut preview_thread: Option<PreviewThread> = None;
    while let Ok(_event) = rx_preview.recv() {
        if preview_thread.is_some() {
            preview_thread.unwrap().kill();
            preview_thread = None;
        }

        let mut new_prv = match _event {
            PreviewEvent::EvPreviewRequest(preview_input) => preview_input,
            PreviewEvent::EvAbort => return,
        };

        // Try to empty the channel. Happens when spamming up/down or typing fast.
        while let Ok(_event) = rx_preview.try_recv() {
            new_prv = match _event {
                PreviewEvent::EvPreviewRequest(preview_input) => preview_input,
                PreviewEvent::EvAbort => return,
            }
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
                callback(vec![astdout]);
                preview_thread = None;
            }
            Ok(spawned) => {
                let pid = spawned.id();
                let stopped = Arc::new(AtomicBool::new(false));
                let stopped_clone = stopped.clone();
                let callback_clone = callback.clone();
                let thread = thread::spawn(move || {
                    wait(spawned, move |lines| {
                        stopped_clone.store(true, Ordering::SeqCst);
                        callback_clone(lines);
                    })
                });
                preview_thread = Some(PreviewThread { pid, thread, stopped });
            }
        }
    }
}

fn wait<C>(spawned: std::process::Child, callback: C)
where
    C: Fn(Vec<AnsiString>),
{
    let output = spawned.wait_with_output();

    if output.is_err() {
        return;
    }

    let output = output.unwrap();

    // Capture stderr in case users want to debug ...
    let out_str = String::from_utf8_lossy(if output.status.success() {
        &output.stdout
    } else {
        &output.stderr
    });

    let lines = out_str.lines().map(AnsiString::from_str).collect();
    callback(lines);
}

#[derive(Builder, Default, Debug)]
#[builder(default)]
struct Printer {
    #[builder(setter(skip))]
    row: usize,
    #[builder(setter(skip))]
    col: usize,
    skip_rows: usize,
    skip_cols: usize,
    wrap: bool,
    width: usize,
    height: usize,
}

impl Printer {
    pub fn print_lines(&mut self, canvas: &mut dyn Canvas, content: &[AnsiString]) {
        for (line_no, line) in content.iter().enumerate() {
            if line_no < self.skip_rows {
                self.move_to_next_line();
                continue;
            } else if self.row >= self.skip_rows + self.height {
                break;
            }

            for (ch, attr) in line.iter() {
                let _ = self.print_char_with_attr(canvas, ch, attr);

                // skip if the content already exceeded the canvas
                if !self.wrap && self.col >= self.width + self.skip_cols {
                    break;
                }

                if self.row >= self.skip_rows + self.height {
                    break;
                }
            }

            self.move_to_next_line();
        }
    }

    fn move_to_next_line(&mut self) {
        self.row += 1;
        self.col = 0;
    }

    fn print_char_with_attr(&mut self, canvas: &mut dyn Canvas, ch: char, attr: Attr) -> Result<()> {
        match ch {
            '\n' | '\r' | '\0' => {}
            '\t' => {
                // handle tabstop
                let rest = TAB_STOP - self.col % TAB_STOP;
                let rest = min(rest, max(self.col, self.width) - self.col);
                for _ in 0..rest {
                    self.print_char_raw(canvas, ' ', attr)?;
                }
            }

            ch => {
                self.print_char_raw(canvas, ch, attr)?;
            }
        }
        Ok(())
    }

    fn print_char_raw(&mut self, canvas: &mut dyn Canvas, ch: char, attr: Attr) -> Result<()> {
        if self.row < self.skip_rows || self.row >= self.height + self.skip_rows {
            return Ok(());
        }

        if self.wrap {
            // if wrap is enabled, hscroll is discarded
            self.col += self.adjust_scroll_print(canvas, ch, attr)?;

            if self.col >= self.width {
                // re-print the wide character
                self.move_to_next_line();
            }

            if self.col > self.width {
                self.col += self.adjust_scroll_print(canvas, ch, attr)?;
            }
        } else {
            self.col += self.adjust_scroll_print(canvas, ch, attr)?;
        }

        Ok(())
    }

    fn adjust_scroll_print(&self, canvas: &mut dyn Canvas, ch: char, attr: Attr) -> Result<usize> {
        if self.row < self.skip_rows || self.col < self.skip_cols {
            canvas.put_char_with_attr(usize::max_value(), usize::max_value(), ch, attr)
        } else {
            canvas.put_char_with_attr(self.row - self.skip_rows, self.col - self.skip_cols, ch, attr)
        }
    }
}

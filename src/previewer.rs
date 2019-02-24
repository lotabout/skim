use crate::event::{Event, EventSender};
use crate::ansi::AnsiString;
use nix::libc;

use std::env;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;

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

pub fn run(rx_preview: Receiver<(Event, PreviewInput)>, tx_model: EventSender) {
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
                tx_model
                    .clone()
                    .send((
                        Event::EvModelNewPreview,
                        Box::new(format!("Failed to spawn: {} / {}", cmd, err)),
                    ))
                    .expect("Failed to send Error msg");
                preview_thread = None;
            }
            Ok(spawned) => {
                let pid = spawned.id();
                let stopped = Arc::new(AtomicBool::new(false));
                let tx_model = tx_model.clone();
                let stopped_c = stopped.clone();
                let thread = thread::spawn(move || wait_and_send(spawned, tx_model, stopped_c));
                preview_thread = Some(PreviewThread { pid, thread, stopped });
            }
        }
    }
}

fn wait_and_send(mut spawned: std::process::Child, tx_model: EventSender, stopped: Arc<AtomicBool>) {
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
    if stdout != "" {
        let astdout = AnsiString::from_str(&stdout);
        tx_model
            .send((Event::EvModelNewPreview, Box::new(astdout)))
            .expect("Failed to send Preview msg");
    }
}

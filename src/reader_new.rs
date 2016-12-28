extern crate libc;

use std::sync::mpsc::{Receiver, Sender, SyncSender, channel};
use std::error::Error;
use item::Item;
use std::sync::{Arc, RwLock};
use std::process::{Command, Stdio, Child};
use std::io::{stdin, BufRead, BufReader};
use event::{Event, EventArg};
use std::thread::{spawn, JoinHandle};
use std::thread;

pub struct Reader {
    rx_cmd: Receiver<(Event, EventArg)>,
    tx_item: SyncSender<Item>,
    items: Arc<RwLock<Vec<Item>>>, // all items
}

impl Reader {
    pub fn new(rx_cmd: Receiver<(Event, EventArg)>, tx_item: SyncSender<Item>) -> Self {
        Reader {
            rx_cmd: rx_cmd,
            tx_item: tx_item,
            items: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn run(&mut self) {
        // event loop
        let mut thread_reader: Option<JoinHandle<()>> = None;
        let mut tx_reader: Option<Sender<bool>> = None;

        while let Ok((ev, arg)) = self.rx_cmd.recv() {
            match ev {
                Event::EvReaderRestart => {
                    // close existing command or file if exists
                    tx_reader.map(|tx| {tx.send(true)});
                    thread_reader.take().map(|thrd| {thrd.join()});

                    // send message to stop existing matcher

                    // start command with new query
                    let cmd = *arg.downcast::<String>().unwrap();
                    let items = self.items.clone();
                    let (tx, rx_reader) = channel();
                    tx_reader = Some(tx);
                    thread::spawn(move || {
                        reader(&cmd, rx_reader, items);
                    });

                    // start sending loop to matcher

                }
                _ => {
                    // do nothing
                }
            }

        }
    }

}

fn get_command_output(cmd: &str) -> Result<(Option<Child>, Box<BufRead>), Box<Error>> {
    let mut command = try!(Command::new("sh")
                       .arg("-c")
                       .arg(cmd)
                       .stdout(Stdio::piped())
                       .stderr(Stdio::null())
                       .spawn());
    let stdout = try!(command.stdout.take().ok_or("command output: unwrap failed".to_owned()));
    Ok((Some(command), Box::new(BufReader::new(stdout))))
}

fn reader(cmd: &str, rx: Receiver<bool>, items: Arc<RwLock<Vec<Item>>>) {
    // start the command
    let istty = unsafe { libc::isatty(libc::STDIN_FILENO as i32) } != 0;

    let (command, mut source): (Option<Child>, Box<BufRead>) = if istty {
        get_command_output(cmd).expect("command not found")
    } else {
        (None, Box::new(BufReader::new(stdin())))
    };

    loop {
        // start reading
        let mut input = String::new();
        match source.read_line(&mut input) {
            Ok(n) => {
                if n == 0 { break; }

                if input.ends_with('\n') {
                    input.pop();
                    if input.ends_with('\r') {
                        input.pop();
                    }
                }
                let mut items = items.write().unwrap();
                items.push(Item::new_plain(input));
            }
            Err(_err) => {} // String not UTF8 or other error, skip.
        }
    }

    // clean up resources
    command.map(|mut x| {
        let _ = x.kill();
        let _ = x.wait();
    });
}

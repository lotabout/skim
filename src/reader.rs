/// Reader will read the entries from stdin or command output
/// And send the entries to controller, the controller will save it into model.

extern crate libc;

use std::process::{Command, Stdio, Child};
use std::sync::{Arc, RwLock};
use std::io::{stdin, BufRead, BufReader};
use std::error::Error;
use util::eventbox::EventBox;
use event::Event;
use item::Item;
use getopts;

const READER_EVENT_DURATION: u64 = 30;

pub struct Reader {
    cmd: String, // command to invoke
    eb: Arc<EventBox<Event>>,         // eventbox
    pub eb_req: Arc<EventBox<Event>>,
    items: Arc<RwLock<Vec<Item>>>, // all items
    use_ansi_color: bool,
}

impl Reader {

    pub fn new(cmd: String, eb: Arc<EventBox<Event>>, items: Arc<RwLock<Vec<Item>>>) -> Self {
        Reader{cmd: cmd,
               eb: eb,
               eb_req: Arc::new(EventBox::new()),
               items: items,
               use_ansi_color: true,
        }
    }

    // invoke find comand.
    fn get_command_output(&self, arg: &str) -> Result<(Option<Child>, Box<BufRead>), Box<Error>> {
        let mut command = try!(Command::new("sh")
                           .arg("-c")
                           .arg(self.cmd.replace("{}", arg))
                           .stdout(Stdio::piped())
                           .stderr(Stdio::null())
                           .spawn());
        let stdout = try!(command.stdout.take().ok_or("command output: unwrap failed".to_owned()));
        Ok((Some(command), Box::new(BufReader::new(stdout))))
    }

    pub fn parse_options(&mut self, options: &getopts::Matches) {
        if let Some(cmd) = options.opt_str("c") {
            self.cmd = cmd.clone();
        }
    }

    pub fn run(&mut self) {
        // check if the input is TTY
        let istty = unsafe { libc::isatty(libc::STDIN_FILENO as i32) } != 0;
        let mut arg = "".to_string();

        loop {
            let (command, read): (Option<Child>, Box<BufRead>) = if istty {
                self.get_command_output(&arg).expect("command not found")
            } else {
                (None, Box::new(BufReader::new(stdin())))
            };

            self.read_items(read);
            command.map(|mut x| {
                let _ = x.kill();
                let _ = x.wait();
            });

            for (e, val) in self.eb_req.wait() {
                match e {
                    Event::EvReaderResetQuery => {
                        let mut items = self.items.write().unwrap();
                        items.clear();
                        arg = *val.downcast::<String>().unwrap();
                        self.eb.set(Event::EvReaderSync, Box::new(true));
                        let _ = self.eb_req.wait_for(Event::EvModelAck);
                    }
                    _ => {}
                }
            }
        }
    }

    fn read_items(&self, mut source: Box<BufRead>) {
        loop {
            let mut input = String::new();
            match source.read_line(&mut input) {
                Ok(n) => {
                    if n <= 0 { break; }

                    if input.ends_with("\n") {
                        input.pop();
                        if input.ends_with("\r") {
                            input.pop();
                        }
                    }
                    let mut items = self.items.write().unwrap();
                    items.push(Item::new(input, self.use_ansi_color));
                }
                Err(_err) => {} // String not UTF8 or other error, skip.
            }
            self.eb.set_throttle(Event::EvReaderNewItem, Box::new(true), READER_EVENT_DURATION);
            if self.eb_req.peek(Event::EvReaderResetQuery) {
                break;
            }
        }
        self.eb.set_throttle(Event::EvReaderNewItem, Box::new(false), READER_EVENT_DURATION);
    }
}


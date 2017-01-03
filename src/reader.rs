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
use regex::Regex;

const READER_EVENT_DURATION: u64 = 30;

pub struct Reader {
    cmd: String, // command to invoke
    eb: Arc<EventBox<Event>>,         // eventbox
    pub eb_req: Arc<EventBox<Event>>,
    items: Arc<RwLock<Vec<Item>>>, // all items
    use_ansi_color: bool,
    default_arg: String,
    transform_fields: Vec<FieldRange>,
    matching_fields: Vec<FieldRange>,
    delimiter: Regex,
    replace_str: String,
}

impl Reader {

    pub fn new(cmd: String, eb: Arc<EventBox<Event>>, items: Arc<RwLock<Vec<Item>>>) -> Self {
        Reader{cmd: cmd,
               eb: eb,
               eb_req: Arc::new(EventBox::new()),
               items: items,
               use_ansi_color: false,
               default_arg: String::new(),
               transform_fields: Vec::new(),
               matching_fields: Vec::new(),
               delimiter: Regex::new(r".*?\t").unwrap(),
               replace_str: "{}".to_string(),
        }
    }

    // invoke find comand.
    fn get_command_output(&self, arg: &str) -> Result<(Option<Child>, Box<BufRead>), Box<Error>> {
        let mut command = try!(Command::new("sh")
                           .arg("-c")
                           .arg(self.cmd.replace(&self.replace_str, arg))
                           .stdout(Stdio::piped())
                           .stderr(Stdio::null())
                           .spawn());
        let stdout = try!(command.stdout.take().ok_or("command output: unwrap failed".to_owned()));
        Ok((Some(command), Box::new(BufReader::new(stdout))))
    }

    pub fn parse_options(&mut self, options: &getopts::Matches) {
        if options.opt_present("ansi") {
            self.use_ansi_color = true;
        }

        if let Some(cmd) = options.opt_str("c") {
            self.cmd = cmd.clone();
        }

        if let Some(query) = options.opt_str("q") {
            self.default_arg = query.to_string();
        }

        if let Some(delimiter) = options.opt_str("d") {
            self.delimiter = Regex::new(&(".*?".to_string() + &delimiter))
                .unwrap_or(Regex::new(r".*?\t").unwrap());
        }

        if let Some(transform_fields) = options.opt_str("with-nth") {
            self.transform_fields = transform_fields.split(',')
                .map(|string| {
                    parse_range(string)
                })
                .filter(|range| range.is_some())
                .map(|range| range.unwrap())
                .collect();
        }

        if let Some(matching_fields) = options.opt_str("nth") {
            self.matching_fields = matching_fields.split(',')
                .map(|string| {
                    parse_range(string)
                })
                .filter(|range| range.is_some())
                .map(|range| range.unwrap())
                .collect();
        }

        if let Some(replace_str) = options.opt_str("I") {
            self.replace_str = replace_str.clone();
        }
    }

    pub fn run(&mut self) {
        // check if the input is TTY
        let istty = unsafe { libc::isatty(libc::STDIN_FILENO as i32) } != 0;
        let mut arg = self.default_arg.clone();

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
                if e == Event::EvReaderResetQuery {
                    let mut items = self.items.write().unwrap();
                    items.clear();
                    arg = *val.downcast::<String>().unwrap();
                    self.eb.set(Event::EvReaderSync, Box::new(true));
                    let _ = self.eb_req.wait_for(Event::EvModelAck);
                }
            }
        }
    }

    fn read_items(&self, mut source: Box<BufRead>) {
        loop {
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
                    let mut items = self.items.write().unwrap();
                    items.push(Item::new(input,
                                         self.use_ansi_color,
                                         &self.transform_fields,
                                         &self.matching_fields,
                                         &self.delimiter));
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


#[derive(PartialEq, Eq, Clone, Debug)]
pub enum FieldRange {
    Single(i64),
    LeftInf(i64),
    RightInf(i64),
    Both(i64, i64),
}

// range: "start..end", end is excluded.
// "0", "0..", "..10", "1..10", etc.
fn parse_range(range: &str) -> Option<FieldRange> {
    use self::FieldRange::*;

    if range == ".." {
        return Some(RightInf(0));
    }

    let range_string: Vec<&str> = range.split("..").collect();
    if range_string.is_empty() || range_string.len() > 2 {
        return None;
    }

    let start = range_string.get(0).and_then(|x| x.parse::<i64>().ok());
    let end = range_string.get(1).and_then(|x| x.parse::<i64>().ok());

    if range_string.len() == 1 {
        return if start.is_none() {None} else {Some(Single(start.unwrap()))};
    }

    if start.is_none() && end.is_none() {
        None
    } else if end.is_none() {
        // 1..
        Some(RightInf(start.unwrap()))
    } else if start.is_none() {
        // ..1
        Some(LeftInf(end.unwrap()))
    } else {
        Some(Both(start.unwrap(), end.unwrap()))
    }
}

#[cfg(test)]
mod test {
    use super::FieldRange::*;
    #[test]
    fn test_parse_range() {
        assert_eq!(super::parse_range("1"), Some(Single(1)));
        assert_eq!(super::parse_range("-1"), Some(Single(-1)));

        assert_eq!(super::parse_range("1.."), Some(RightInf(1)));
        assert_eq!(super::parse_range("-1.."), Some(RightInf(-1)));

        assert_eq!(super::parse_range("..1"), Some(LeftInf(1)));
        assert_eq!(super::parse_range("..-1"), Some(LeftInf(-1)));

        assert_eq!(super::parse_range("1..3"), Some(Both(1, 3)));
        assert_eq!(super::parse_range("-1..-3"), Some(Both(-1, -3)));

        assert_eq!(super::parse_range(".."), Some(RightInf(0)));
        assert_eq!(super::parse_range("a.."), None);
        assert_eq!(super::parse_range("..b"), None);
        assert_eq!(super::parse_range("a..b"), None);
    }
}

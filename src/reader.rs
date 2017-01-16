extern crate libc;

use std::sync::mpsc::{Receiver, Sender, SyncSender, channel};
use std::error::Error;
use item::Item;
use std::sync::{Arc, RwLock};
use std::process::{Command, Stdio, Child};
use std::io::{stdin, BufRead, BufReader};
use event::{Event, EventArg};
use std::thread::JoinHandle;
use std::thread;
use std::time::Duration;
use std::collections::HashMap;
use std::mem;

use std::io::Write;
use getopts;
use regex::Regex;
use sender::CachedSender;

macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

struct ReaderOption {
    pub use_ansi_color: bool,
    pub default_arg: String,
    pub transform_fields: Vec<FieldRange>,
    pub matching_fields: Vec<FieldRange>,
    pub delimiter: Regex,
    pub replace_str: String,
}

impl ReaderOption {
    pub fn new() -> Self {
        ReaderOption {
            use_ansi_color: false,
            default_arg: String::new(),
            transform_fields: Vec::new(),
            matching_fields: Vec::new(),
            delimiter: Regex::new(r".*?\t").unwrap(),
            replace_str: "{}".to_string(),
        }
    }

    pub fn parse_options(&mut self, options: &getopts::Matches) {
        if options.opt_present("ansi") {
            self.use_ansi_color = true;
        }

        if let Some(delimiter) = options.opt_str("d") {
            self.delimiter = Regex::new(&(".*?".to_string() + &delimiter))
                .unwrap_or(Regex::new(r".*?\t").unwrap());
        }

        if let Some(transform_fields) = options.opt_str("with-nth") {
            self.transform_fields = transform_fields.split(',')
                .filter_map(|string| {
                    parse_range(string)
                })
                .collect();
        }

        if let Some(matching_fields) = options.opt_str("nth") {
            self.matching_fields = matching_fields.split(',')
                .filter_map(|string| {
                    parse_range(string)
                })
                .collect();
        }
    }
}

pub struct Reader {
    rx_cmd: Receiver<(Event, EventArg)>,
    tx_item: SyncSender<(Event, EventArg)>,
    option: Arc<RwLock<ReaderOption>>,
}

impl Reader {
    pub fn new(rx_cmd: Receiver<(Event, EventArg)>, tx_item: SyncSender<(Event, EventArg)>) -> Self {
        Reader {
            rx_cmd: rx_cmd,
            tx_item: tx_item,
            option: Arc::new(RwLock::new(ReaderOption::new()))
        }
    }

    pub fn parse_options(&mut self, options: &getopts::Matches) {
        let mut option = self.option.write().unwrap();
        option.parse_options(options);
    }

    pub fn run(&mut self) {
        // event loop
        let mut thread_reader: Option<JoinHandle<()>> = None;
        let mut tx_reader: Option<Sender<bool>> = None;

        let mut last_command = "".to_string();
        let mut last_query = "".to_string();

        // start sender
        let (tx_sender, rx_sender) = channel();
        let tx_item = self.tx_item.clone();
        let mut sender = CachedSender::new(rx_sender, tx_item);
        thread::spawn(move || {
            sender.run();
        });

        while let Ok((ev, arg)) = self.rx_cmd.recv() {
            match ev {
                Event::EvReaderRestart => {
                    // close existing command or file if exists
                    let (cmd, query) = *arg.downcast::<(String, String)>().unwrap();
                    if cmd == last_command && query == last_query { continue; }

                    // restart command with new `command`
                    if cmd != last_command {
                        // stop existing command
                        tx_reader.take().map(|tx| {tx.send(true)});
                        thread_reader.take().map(|thrd| {thrd.join()});

                        // create needed data for thread
                        let (tx, rx_reader) = channel();
                        tx_reader = Some(tx);
                        let cmd_clone = cmd.clone();
                        let option_clone = self.option.clone();
                        let tx_sender_clone = tx_sender.clone();
                        let query_clone = query.clone();

                        // start the new command
                        thread_reader = Some(thread::spawn(move || {
                            let _ = tx_sender_clone.send((Event::EvReaderStarted, Box::new(true)));
                            let _ = tx_sender_clone.send((Event::EvSenderRestart, Box::new(query_clone)));

                            reader(&cmd_clone, rx_reader, &tx_sender_clone, option_clone);

                            let _ = tx_sender_clone.send((Event::EvReaderStopped, Box::new(true)));
                        }));
                    } else {
                        // tell sender to restart
                        let _ = tx_sender.send((Event::EvSenderRestart, Box::new(query.clone())));
                    }

                    last_command = cmd;
                    last_query = query;
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

// Consider that you invoke a command with different arguments several times
// If you select some items each time, how will skim remeber it?
// => Well, we'll give each invokation a number, i.e. RUN_NUM
// What if you invoke the same command and same arguments twice?
// => We use NUM_MAP to specify the same run number.
lazy_static! {
    static ref RUN_NUM: RwLock<usize> = RwLock::new(0);
    static ref NUM_MAP: RwLock<HashMap<String, usize>> = RwLock::new(HashMap::new());
}

fn reader(cmd: &str,
          rx_cmd: Receiver<bool>,
          tx_sender: &Sender<(Event, EventArg)>,
          option: Arc<RwLock<ReaderOption>>) {
    let istty = unsafe { libc::isatty(libc::STDIN_FILENO as i32) } != 0;

    let (command, mut source): (Option<Child>, Box<BufRead>) = if istty {
        get_command_output(cmd).expect("command not found")
    } else {
        (None, Box::new(BufReader::new(stdin())))
    };

    let (tx_control, rx_control) = channel();

    thread::spawn(move || {
        // listen to `rx` for command to quit reader
        // kill command if it is got
        loop {
            if rx_cmd.try_recv().is_ok() {
                // clean up resources
                command.map(|mut x| {
                    let _ = x.kill();
                    let _ = x.wait();
                });
                break;
            }

            if rx_control.recv_timeout(Duration::from_millis(10)).is_ok() {
                command.map(|mut x| {
                    let _ = x.kill();
                    let _ = x.wait();
                });
                break;
            }
        }
    });

    let opt = option.read().unwrap();

    // set the proper run number
    let run_num = {*RUN_NUM.read().unwrap()};
    let run_num = *NUM_MAP.write()
            .unwrap()
            .entry(cmd.to_string())
            .or_insert_with(|| {
                *(RUN_NUM.write().unwrap()) = run_num + 1;
                run_num + 1
            });

    let mut index = 0;
    let mut item_group = Vec::new();
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
                let item = Item::new(input,
                                     opt.use_ansi_color,
                                     &opt.transform_fields,
                                     &opt.matching_fields,
                                     &opt.delimiter,
                                     (run_num, index));
                item_group.push(Arc::new(item));
                index += 1;

                if index & 0xFFF == 0 {
                    let _ = tx_sender.send((Event::EvReaderNewItem, Box::new(mem::replace(&mut item_group, Vec::new()))));
                }
            }
            Err(_err) => {} // String not UTF8 or other error, skip.
        }
    }

    if !item_group.is_empty() {
        let _ = tx_sender.send((Event::EvReaderNewItem, Box::new(mem::replace(&mut item_group, Vec::new()))));
    }

    let _ = tx_control.send(true);
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

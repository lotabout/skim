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
use std::time::Duration;
use std::collections::HashMap;

use std::io::Write;
use getopts;
use regex::Regex;

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
    }
}

pub struct Reader {
    rx_cmd: Receiver<(Event, EventArg)>,
    tx_item: SyncSender<(Event, EventArg)>,
    items: Arc<RwLock<Vec<Item>>>, // all items
    option: Arc<RwLock<ReaderOption>>,
}

impl Reader {
    pub fn new(rx_cmd: Receiver<(Event, EventArg)>, tx_item: SyncSender<(Event, EventArg)>) -> Self {
        Reader {
            rx_cmd: rx_cmd,
            tx_item: tx_item,
            items: Arc::new(RwLock::new(Vec::new())),
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
        let mut thread_sender: Option<JoinHandle<()>> = None;
        let mut tx_reader: Option<Sender<bool>> = None;
        let mut tx_sender: Option<Sender<bool>> = None;
        let mut last_command = "".to_string();

        let reader_stopped = Arc::new(RwLock::new(false));

        while let Ok((ev, arg)) = self.rx_cmd.recv() {
            match ev {
                Event::EvReaderRestart => {
                    // close existing command or file if exists
                    let (cmd, query) = *arg.downcast::<(String, String)>().unwrap();

                    // send message to stop existing matcher
                    tx_sender.map(|tx| {tx.send(true)});
                    thread_sender.take().map(|thrd| {thrd.join()});

                    // restart command with new `command`
                    if cmd != last_command {
                        // stop existing command
                        tx_reader.take().map(|tx| {tx.send(true)});
                        thread_reader.take().map(|thrd| {thrd.join()});

                        {
                            // remove existing items
                            let mut items = self.items.write().unwrap();
                            items.clear();
                        }

                        // start new command
                        let items = self.items.clone();
                        let (tx, rx_reader) = channel();
                        tx_reader = Some(tx);
                        let cmd_clone = cmd.clone();
                        let option_clone = self.option.clone();
                        let tx_item = self.tx_item.clone();
                        let reader_stopped_clone = reader_stopped.clone();
                        thread::spawn(move || {
                            {*reader_stopped_clone.write().unwrap() = false;}
                            tx_item.send((Event::EvReaderStopped, Box::new(false)));

                            reader(&cmd_clone, rx_reader, items, option_clone);

                            tx_item.send((Event::EvReaderStopped, Box::new(true)));
                            {*reader_stopped_clone.write().unwrap() = true;}
                        });
                    }

                    // start "sending loop" to matcher
                    let tx_item = self.tx_item.clone();
                    let items = self.items.clone();
                    let (tx, rx_sender) = channel();
                    tx_sender = Some(tx);
                    let reader_stopped_clone = reader_stopped.clone();
                    thread::spawn(move || {
                        // tell matcher that reader will start to send new items.
                        tx_item.send((Event::EvMatcherRestart, Box::new(query)));
                        sender(rx_sender, &tx_item, items, reader_stopped_clone);
                        tx_item.send((Event::EvSenderStopped, Box::new(true)));
                    });

                    last_command = cmd;
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
          items: Arc<RwLock<Vec<Item>>>,
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
            if let Ok(quit) = rx_cmd.try_recv() {
                // clean up resources
                command.map(|mut x| {
                    let _ = x.kill();
                    let _ = x.wait();
                });
                break;
            }

            if let Ok(quit) = rx_control.recv_timeout(Duration::from_millis(10)) {
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
    let mut run_num = {*RUN_NUM.read().unwrap()};
    let run_num = *NUM_MAP.write()
            .unwrap()
            .entry(cmd.to_string())
            .or_insert_with(|| {
                *(RUN_NUM.write().unwrap()) = run_num + 1;
                run_num + 1
            });

    let mut index = 0;
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
                items.push(Item::new(input,
                                     opt.use_ansi_color,
                                     &opt.transform_fields,
                                     &opt.matching_fields,
                                     &opt.delimiter,
                                     (run_num, index)));

                index += 1;
            }
            Err(_err) => {} // String not UTF8 or other error, skip.
        }
    }
    tx_control.send(true);
}

fn sender(rx_cmd: Receiver<bool>,
          tx: &SyncSender<(Event, EventArg)>,
          items: Arc<RwLock<Vec<Item>>>,
          reader_stopped: Arc<RwLock<bool>>) {
    let mut index = 0;
    loop {
        if let Ok(quit) = rx_cmd.try_recv() {
            break;
        }

        let all_read;
        {
            let items = items.read().unwrap();
            all_read = index >= items.len();
            if !all_read {
                tx.send((Event::EvMatcherNewItem, Box::new(items[index].clone())));
                index += 1;
            }
        }

        if *reader_stopped.read().unwrap() && all_read {
            break;
        } else if all_read {
            thread::sleep(Duration::from_millis(1));
        }
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

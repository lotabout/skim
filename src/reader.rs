use std::sync::mpsc::{Receiver, Sender, SyncSender, channel};
use std::error::Error;
use item::Item;
use std::sync::{Arc, RwLock};
use std::process::{Command, Stdio, Child};
use std::io::{BufRead, BufReader};
use event::{Event, EventArg};
use std::thread::JoinHandle;
use std::thread;
use std::time::Duration;
use std::collections::HashMap;
use std::mem;
use std::fs::File;

use regex::Regex;
use sender::CachedSender;
use field::{FieldRange, parse_range};
use clap::ArgMatches;

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

    pub fn parse_options(&mut self, options: &ArgMatches) {
        if options.is_present("ansi") {
            self.use_ansi_color = true;
        }

        if let Some(delimiter) = options.value_of("delimiter") {
            self.delimiter = Regex::new(&(".*?".to_string() + delimiter))
                .unwrap_or_else(|_| Regex::new(r".*?[\t ]").unwrap());
        }

        if let Some(transform_fields) = options.value_of("with-nth") {
            self.transform_fields = transform_fields.split(',')
                .filter_map(|string| {
                    parse_range(string)
                })
                .collect();
        }

        if let Some(matching_fields) = options.value_of("nth") {
            self.matching_fields = matching_fields.split(',')
                .filter_map(|string| {
                    parse_range(string)
                }).collect();
        }
    }
}

pub struct Reader {
    rx_cmd: Receiver<(Event, EventArg)>,
    tx_item: SyncSender<(Event, EventArg)>,
    option: Arc<RwLock<ReaderOption>>,
    real_stdin: Option<File>,  // used to support piped output
}

impl Reader {
    pub fn new(rx_cmd: Receiver<(Event, EventArg)>,
               tx_item: SyncSender<(Event, EventArg)>,
               real_stdin: Option<File>) -> Self {
        Reader {
            rx_cmd: rx_cmd,
            tx_item: tx_item,
            option: Arc::new(RwLock::new(ReaderOption::new())),
            real_stdin,
        }
    }

    pub fn parse_options(&mut self, options: &ArgMatches) {
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
                    let (cmd, query, force_update) = *arg.downcast::<(String, String, bool)>().unwrap();
                    if !force_update && cmd == last_command && query == last_query { continue; }

                    // restart command with new `command`
                    if cmd != last_command {
                        // stop existing command
                        tx_reader.take().map(|tx| {tx.send(true)});
                        thread_reader.take().map(|thrd| {thrd.join()});

                        // create needed data for thread
                        let (tx, rx_reader) = channel();
                        tx_reader = Some(tx);
                        let cmd_clone = cmd.clone();
                        let option_clone = Arc::clone(&self.option);
                        let tx_sender_clone = tx_sender.clone();
                        let query_clone = query.clone();
                        let real_stdin = self.real_stdin.take();

                        // start the new command
                        thread_reader = Some(thread::spawn(move || {
                            let _ = tx_sender_clone.send((Event::EvReaderStarted, Box::new(true)));
                            let _ = tx_sender_clone.send((Event::EvSenderRestart, Box::new(query_clone)));

                            reader(&cmd_clone, rx_reader, &tx_sender_clone, option_clone, real_stdin);

                            let _ = tx_sender_clone.send((Event::EvReaderStopped, Box::new(true)));
                        }));
                    } else {
                        // tell sender to restart
                        let _ = tx_sender.send((Event::EvSenderRestart, Box::new(query.clone())));
                    }

                    last_command = cmd;
                    last_query = query;
                }

                Event::EvActAccept => {
                    // stop existing command
                    tx_reader.take().map(|tx| {tx.send(true)});
                    thread_reader.take().map(|thrd| {thrd.join()});
                    let tx_ack: Sender<usize> = *arg.downcast().unwrap();
                    let _ = tx_ack.send(0);
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
    let stdout = try!(command.stdout.take().ok_or_else(|| "command output: unwrap failed".to_owned()));
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
          option: Arc<RwLock<ReaderOption>>,
          source_file: Option<File>) {

    debug!("reader:reader: called");
    let (command, mut source): (Option<Child>, Box<BufRead>) = if source_file.is_some() {
        (None, Box::new(BufReader::new(source_file.unwrap())))
    } else {
        get_command_output(cmd).expect("command not found")
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
                debug!("reader:reader: read a new line. index = {}", index);

                if input.ends_with('\n') {
                    input.pop();
                    if input.ends_with('\r') {
                        input.pop();
                    }
                }
                debug!("reader:reader: create new item. index = {}", index);
                let item = Item::new(input,
                                     opt.use_ansi_color,
                                     &opt.transform_fields,
                                     &opt.matching_fields,
                                     &opt.delimiter,
                                     (run_num, index));
                item_group.push(Arc::new(item));
                debug!("reader:reader: item created. index = {}", index);
                index += 1;

                // % 4096 == 0
                if index.trailing_zeros() > 12 {
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


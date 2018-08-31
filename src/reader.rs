use event::{Event, EventArg, EventReceiver, EventSender};
use item::Item;
use std::collections::HashMap;
use std::error::Error;
use std::io::{BufRead, BufReader};
use std::mem;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender, SyncSender};
use std::sync::{Arc, RwLock};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use field::FieldRange;
use options::SkimOptions;
use regex::Regex;
use sender::CachedSender;

struct ReaderOption {
    pub use_ansi_color: bool,
    pub default_arg: String,
    pub transform_fields: Vec<FieldRange>,
    pub matching_fields: Vec<FieldRange>,
    pub delimiter: Regex,
    pub replace_str: String,
    pub line_ending: u8,
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
            line_ending: b'\n',
        }
    }

    pub fn parse_options(&mut self, options: &SkimOptions) {
        if options.ansi {
            self.use_ansi_color = true;
        }

        if let Some(delimiter) = options.delimiter {
            self.delimiter =
                Regex::new(&(".*?".to_string() + delimiter)).unwrap_or_else(|_| Regex::new(r".*?[\t ]").unwrap());
        }

        if let Some(transform_fields) = options.with_nth {
            self.transform_fields = transform_fields
                .split(',')
                .filter_map(|string| FieldRange::from_str(string))
                .collect();
        }

        if let Some(matching_fields) = options.nth {
            self.matching_fields = matching_fields
                .split(',')
                .filter_map(|string| FieldRange::from_str(string))
                .collect();
        }

        if options.read0 {
            self.line_ending = b'\0';
        }
    }
}

pub struct Reader {
    rx_cmd: EventReceiver,
    tx_item: SyncSender<(Event, EventArg)>,
    option: Arc<RwLock<ReaderOption>>,
    data_source: Option<Box<BufRead + Send>>, // used to support piped output
}

impl Reader {
    pub fn new(
        rx_cmd: EventReceiver,
        tx_item: SyncSender<(Event, EventArg)>,
        data_source: Option<Box<BufRead + Send>>,
    ) -> Self {
        Reader {
            rx_cmd: rx_cmd,
            tx_item: tx_item,
            option: Arc::new(RwLock::new(ReaderOption::new())),
            data_source,
        }
    }

    pub fn parse_options(&mut self, options: &SkimOptions) {
        let mut option = self.option
            .write()
            .expect("reader:parse_options: failed to lock option");
        option.parse_options(options);
    }

    pub fn run(&mut self) {
        // event loop
        let mut thread_reader: Option<JoinHandle<()>> = None;
        let reader_stopped: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

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
                    let (cmd, query, force_update) = *arg.downcast::<(String, String, bool)>()
                        .expect("reader:EvReaderRestart: failed to get argument");
                    if !force_update && cmd == last_command && query == last_query {
                        continue;
                    }

                    // restart command with new `command`
                    if cmd != last_command {
                        // stop existing command
                        reader_stopped.store(true, Ordering::SeqCst);
                        thread_reader.take().map(|thrd| thrd.join());

                        // create needed data for thread
                        reader_stopped.store(false, Ordering::SeqCst);
                        let cmd_clone = cmd.clone();
                        let option_clone = Arc::clone(&self.option);
                        let tx_sender_clone = tx_sender.clone();
                        let query_clone = query.clone();
                        let data_source = self.data_source.take();
                        let stopped = reader_stopped.clone();

                        // start the new command
                        thread_reader = Some(thread::spawn(move || {
                            let _ = tx_sender_clone.send((Event::EvReaderStarted, Box::new(true)));
                            let _ = tx_sender_clone.send((Event::EvSenderRestart, Box::new(query_clone)));

                            reader(&cmd_clone, stopped, &tx_sender_clone, option_clone, data_source);

                            let _ = tx_sender_clone.send((Event::EvReaderStopped, Box::new(true)));
                        }));
                    } else {
                        // tell sender to restart
                        let _ = tx_sender.send((Event::EvSenderRestart, Box::new(query.clone())));
                    }

                    last_command = cmd;
                    last_query = query;
                }

                ev @ Event::EvActAccept | ev @ Event::EvActAbort => {
                    // stop existing command
                    reader_stopped.store(true, Ordering::SeqCst);
                    thread_reader.take().map(|thrd| thrd.join());
                    let tx_ack: Sender<usize> = *arg.downcast().expect("reader:EvActAccept: failed to get argument");
                    let _ = tx_ack.send(0);

                    // pass the event to sender
                    let _ = tx_sender.send((ev, Box::new(true)));

                    // quit the loop
                    break;
                }

                _ => {
                    // do nothing
                }
            }
        }
    }
}

fn get_command_output(cmd: &str) -> Result<(Option<Child>, Box<BufRead + Send>), Box<Error>> {
    let mut command = try!(
        Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
    );
    let stdout = try!(
        command
            .stdout
            .take()
            .ok_or_else(|| "command output: unwrap failed".to_owned())
    );
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

fn reader(
    cmd: &str,
    stopped: Arc<AtomicBool>,
    tx_sender: &EventSender,
    option: Arc<RwLock<ReaderOption>>,
    source_file: Option<Box<BufRead + Send>>,
) {
    debug!("reader:reader: called");
    let (command, mut source) = source_file
        .map(|f| (None, f))
        .unwrap_or_else(|| get_command_output(cmd).expect("command not found"));

    let stopped_clone = stopped.clone();
    thread::spawn(move || {
        // listen to `rx` for command to quit reader
        // kill command if it is got
        loop {
            if stopped_clone.load(Ordering::Relaxed) {
                // clean up resources
                command.map(|mut x| {
                    let _ = x.kill();
                    let _ = x.wait();
                });
                break;
            }

            thread::sleep(Duration::from_millis(5));
        }
    });

    let opt = option.read().expect("reader: failed to lock option");

    // set the proper run number
    let run_num = { *RUN_NUM.read().expect("reader: failed to lock RUN_NUM") };
    let run_num = *NUM_MAP
        .write()
        .expect("reader: failed to lock NUM_MAP")
        .entry(cmd.to_string())
        .or_insert_with(|| {
            *(RUN_NUM.write().expect("reader: failed to lock RUN_NUM for write")) = run_num + 1;
            run_num + 1
        });

    let mut index = 0;
    let mut item_group = Vec::new();
    let mut buffer = Vec::with_capacity(100);
    loop {
        buffer.clear();
        // start reading
        match source.read_until(opt.line_ending, &mut buffer) {
            Ok(n) => {
                if n == 0 {
                    break;
                }
                debug!("reader:reader: read a new line. index = {}", index);

                if buffer.ends_with(&[b'\r', b'\n']) {
                    buffer.pop();
                    buffer.pop();
                } else if buffer.ends_with(&[b'\n']) || buffer.ends_with(&[b'\0']) {
                    buffer.pop();
                }

                debug!("reader:reader: create new item. index = {}", index);
                let item = Item::new(
                    String::from_utf8_lossy(&buffer),
                    opt.use_ansi_color,
                    &opt.transform_fields,
                    &opt.matching_fields,
                    &opt.delimiter,
                    (run_num, index),
                );
                item_group.push(Arc::new(item));
                //debug!("reader:reader: item created. index = {}", index);
                index += 1;

                // % 516 == 0
                if index.trailing_zeros() > 10 {
                    let _ = tx_sender.send((
                        Event::EvReaderNewItem,
                        Box::new(mem::replace(&mut item_group, Vec::new())),
                    ));
                }

                if index.trailing_zeros() > 5 {
                    if stopped.load(Ordering::SeqCst) {
                        break;
                    }
                }
            }
            Err(_err) => {} // String not UTF8 or other error, skip.
        }
    }

    if !item_group.is_empty() {
        let _ = tx_sender.send((
            Event::EvReaderNewItem,
            Box::new(mem::replace(&mut item_group, Vec::new())),
        ));
    }

    stopped.store(true, Ordering::Relaxed);
}

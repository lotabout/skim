///! Reader is used for reading items from datasource (e.g. stdin or command output)
///!
///! After reading in a line, reader will save an item into the pool(items)
use crate::field::FieldRange;
use crate::item::{ItemWrapper, DefaultSkimItem};
use crate::options::SkimOptions;
use crate::spinlock::SpinLock;
use crate::SkimItem;
use crossbeam::channel::{bounded, select, Receiver, Sender};
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;

const DELIMITER_STR: &str = r"[\t\n ]+";
const CHANNEL_SIZE: usize = 1024;

pub struct ReaderControl {
    tx_interrupt: Sender<i32>,
    components_to_stop: Arc<AtomicUsize>,
    items: Arc<SpinLock<Vec<Arc<ItemWrapper>>>>,
}

impl ReaderControl {
    pub fn kill(self) {
        debug!(
            "kill reader, components before: {}",
            self.components_to_stop.load(Ordering::SeqCst)
        );
        let _ = self.tx_interrupt.send(1);
        while self.components_to_stop.load(Ordering::SeqCst) != 0 {
            //
        }
    }

    pub fn take(&self) -> Vec<Arc<ItemWrapper>> {
        let mut items = self.items.lock();
        let mut ret = Vec::with_capacity(items.len());
        ret.append(&mut items);
        ret
    }

    pub fn is_done(&self) -> bool {
        let items = self.items.lock();
        self.components_to_stop.load(Ordering::SeqCst) == 0 && items.is_empty()
    }
}

pub struct Reader {
    option: Arc<ReaderOption>,
    rx_item: Option<Receiver<Arc<dyn SkimItem>>>,
}

impl Reader {
    pub fn with_options(options: &SkimOptions) -> Self {
        Self {
            option: Arc::new(ReaderOption::with_options(&options)),
            rx_item: None,
        }
    }

    pub fn source(mut self, rx_item: Option<Receiver<Arc<dyn SkimItem>>>) -> Self {
        self.rx_item = rx_item;
        self
    }

    pub fn run(&mut self, cmd: &str) -> ReaderControl {
        let components_to_stop: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let components_to_stop_clone = components_to_stop.clone();
        let items = Arc::new(SpinLock::new(Vec::new()));
        let items_clone = items.clone();
        let option_clone = self.option.clone();
        let cmd = cmd.to_string();

        let (tx_interrupt, rx_interrupt) = bounded(CHANNEL_SIZE);

        match self.rx_item.take() {
            Some(rx_item) => {
                // read item from the channel
                thread::spawn(move || {
                    collect_item(components_to_stop_clone, rx_interrupt, rx_item, items_clone);
                });
            }
            None => {
                // invoke command and read from its output
                let tx_interrupt_clone = tx_interrupt.clone();
                thread::spawn(move || {
                    read_and_collect_from_command(
                        components_to_stop_clone,
                        tx_interrupt_clone,
                        rx_interrupt,
                        &cmd,
                        option_clone,
                        items_clone,
                    );
                });
            }
        }

        ReaderControl {
            tx_interrupt,
            components_to_stop,
            items,
        }
    }
}

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
            delimiter: Regex::new(DELIMITER_STR).unwrap(),
            replace_str: "{}".to_string(),
            line_ending: b'\n',
        }
    }

    pub fn with_options(options: &SkimOptions) -> Self {
        let mut reader_option = Self::new();
        reader_option.parse_options(&options);
        reader_option
    }

    fn parse_options(&mut self, options: &SkimOptions) {
        if options.ansi {
            self.use_ansi_color = true;
        }

        if let Some(delimiter) = options.delimiter {
            self.delimiter = Regex::new(delimiter).unwrap_or_else(|_| Regex::new(DELIMITER_STR).unwrap());
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

// Consider that you invoke a command with different arguments several times
// If you select some items each time, how will skim remember it?
// => Well, we'll give each invocation a number, i.e. RUN_NUM
// What if you invoke the same command and same arguments twice?
// => We use NUM_MAP to specify the same run number.
lazy_static! {
    static ref RUN_NUM: AtomicUsize = AtomicUsize::new(0);
    static ref NUM_MAP: RwLock<HashMap<String, usize>> = RwLock::new(HashMap::new());
}

fn collect_item(
    components_to_stop: Arc<AtomicUsize>,
    rx_interrupt: Receiver<i32>,
    rx_item: Receiver<Arc<dyn SkimItem>>,
    items: Arc<SpinLock<Vec<Arc<ItemWrapper>>>>,
) {
    debug!("reader: collect_item start");
    components_to_stop.fetch_add(1, Ordering::SeqCst);

    let run_num = RUN_NUM.load(Ordering::SeqCst);
    let mut index = 0;
    loop {
        select! {
            recv(rx_item) -> new_item => match new_item {
                Ok(item) => {
                    let item_wrapped = ItemWrapper::new(item, (run_num, index));
                    let mut vec = items.lock();
                    vec.push(Arc::new(item_wrapped));
                    index += 1;
                }
                Err(_) => break,
            },
            recv(rx_interrupt) -> _msg => break,
        }
    }

    components_to_stop.fetch_sub(1, Ordering::SeqCst);
    debug!("reader: collect_item stop");
}

fn read_and_collect_from_command(
    components_to_stop: Arc<AtomicUsize>,
    tx_interrupt: Sender<i32>,
    rx_interrupt: Receiver<i32>,

    cmd: &str,
    option: Arc<ReaderOption>,
    items: Arc<SpinLock<Vec<Arc<ItemWrapper>>>>,
) {
    let (mut command, mut source) = get_command_output(cmd).expect("command not found");

    let components_to_stop_clone = components_to_stop.clone();
    // listening to close signal and kill command if needed
    thread::spawn(move || {
        debug!("reader: command killer start");
        components_to_stop_clone.fetch_add(1, Ordering::SeqCst);

        let _ = rx_interrupt.recv(); // block waiting
        let _ = command.kill();
        let _ = command.wait();

        components_to_stop_clone.fetch_sub(1, Ordering::SeqCst);
        debug!("reader: command killer stop");
    });

    debug!("reader: command reader start");
    components_to_stop.fetch_add(1, Ordering::SeqCst);

    let opt = option;
    // set the proper run number
    let run_num = *NUM_MAP
        .write()
        .expect("reader: failed to lock NUM_MAP")
        .entry(cmd.to_string())
        .or_insert_with(|| RUN_NUM.fetch_add(1, Ordering::SeqCst));

    let mut index = 0;
    let mut buffer = Vec::with_capacity(1024);
    loop {
        buffer.clear();
        // start reading
        match source.read_until(opt.line_ending, &mut buffer) {
            Ok(n) => {
                if n == 0 {
                    break;
                }

                if buffer.ends_with(&[b'\r', b'\n']) {
                    buffer.pop();
                    buffer.pop();
                } else if buffer.ends_with(&[b'\n']) || buffer.ends_with(&[b'\0']) {
                    buffer.pop();
                }

                let raw_item = DefaultSkimItem::new(
                    String::from_utf8_lossy(&buffer),
                    opt.use_ansi_color,
                    &opt.transform_fields,
                    &opt.matching_fields,
                    &opt.delimiter,
                );

                let item = ItemWrapper::new(Arc::new(raw_item), (run_num, index));

                {
                    // save item into pool
                    let mut vec = items.lock();
                    vec.push(Arc::new(item));
                    index += 1;
                }
            }
            Err(_err) => {} // String not UTF8 or other error, skip.
        }
    }

    let _ = tx_interrupt.send(1); // ensure the waiting thread will exit
    components_to_stop.fetch_sub(1, Ordering::SeqCst);
    debug!("reader: command reader stop");
}

type CommandOutput = (Child, Box<dyn BufRead + Send>);
fn get_command_output(cmd: &str) -> Result<CommandOutput, Box<dyn Error>> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let mut command = Command::new(shell)
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = command
        .stdout
        .take()
        .ok_or_else(|| "command output: unwrap failed".to_owned())?;

    Ok((command, Box::new(BufReader::new(stdout))))
}

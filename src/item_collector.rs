use crate::field::FieldRange;
use crate::item::DefaultSkimItem;
use crate::{SkimItem, SkimItemReceiver, SkimItemSender, SkimOptions};
use crossbeam::channel::{bounded, Receiver, Sender};
use regex::Regex;
use std::env;
use std::error::Error;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

const CMD_CHANNEL_SIZE: usize = 1024;
const ITEM_CHANNEL_SIZE: usize = 10240;
const DELIMITER_STR: &str = r"[\t\n ]+";
const READ_BUFFER_SIZE: usize = 1024;

#[derive(Clone, Debug)]
pub struct CollectorOption {
    pub use_ansi_color: bool,
    pub default_arg: String,
    pub transform_fields: Vec<FieldRange>,
    pub matching_fields: Vec<FieldRange>,
    pub delimiter: Regex,
    pub replace_str: String,
    pub line_ending: u8,
}

impl Default for CollectorOption {
    fn default() -> Self {
        Self {
            use_ansi_color: false,
            default_arg: String::new(),
            transform_fields: Vec::new(),
            matching_fields: Vec::new(),
            delimiter: Regex::new(DELIMITER_STR).unwrap(),
            replace_str: "{}".to_string(),
            line_ending: b'\n',
        }
    }
}

impl CollectorOption {
    pub fn with_options(options: &SkimOptions) -> Self {
        let mut reader_option = Self::default();
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

pub enum CollectorInput {
    Pipe(Box<dyn BufRead + Send>),
    Command(String),
}

/// components_to_stop == 0 => all the threads have been stopped
/// return (channel_for_receive_item, channel_to_stop_command)
pub fn read_and_collect_from_command(
    components_to_stop: Arc<AtomicUsize>,
    input: CollectorInput,
    option: CollectorOption,
) -> (Receiver<Arc<dyn SkimItem>>, Sender<i32>) {
    let (command, mut source) = match input {
        CollectorInput::Pipe(pipe) => (None, pipe),
        CollectorInput::Command(cmd) => get_command_output(&cmd).expect("command not found"),
    };

    let (tx_interrupt, rx_interrupt) = bounded(CMD_CHANNEL_SIZE);
    let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) = bounded(ITEM_CHANNEL_SIZE);

    let started = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();
    let components_to_stop_clone = components_to_stop.clone();
    // listening to close signal and kill command if needed
    thread::spawn(move || {
        debug!("collector: command killer start");
        components_to_stop_clone.fetch_add(1, Ordering::SeqCst);
        started_clone.store(true, Ordering::SeqCst); // notify parent that it is started

        let _ = rx_interrupt.recv(); // block waiting
                                     // clean up resources
        if let Some(mut x) = command {
            let _ = x.kill();
            let _ = x.wait();
        }

        components_to_stop_clone.fetch_sub(1, Ordering::SeqCst);
        debug!("collector: command killer stop");
    });

    while !started.load(Ordering::SeqCst) {
        // busy waiting for the thread to start. (components_to_stop is added)
    }

    let started = Arc::new(AtomicBool::new(false));
    let started_clone = started.clone();
    let tx_interrupt_clone = tx_interrupt.clone();
    thread::spawn(move || {
        debug!("collector: command collector start");
        components_to_stop.fetch_add(1, Ordering::SeqCst);
        started_clone.store(true, Ordering::SeqCst); // notify parent that it is started

        let opt = option;
        // set the proper run number

        let mut buffer = Vec::with_capacity(READ_BUFFER_SIZE);
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

                    let line = String::from_utf8_lossy(&buffer).to_string();

                    let raw_item = DefaultSkimItem::new(
                        line,
                        opt.use_ansi_color,
                        &opt.transform_fields,
                        &opt.matching_fields,
                        &opt.delimiter,
                    );

                    match tx_item.send(Arc::new(raw_item)) {
                        Ok(_) => {}
                        Err(_) => {
                            debug!("collector: failed to send item, quit");
                            break;
                        }
                    }
                }
                Err(_err) => {} // String not UTF8 or other error, skip.
            }
        }

        let _ = tx_interrupt_clone.send(1); // ensure the waiting thread will exit
        components_to_stop.fetch_sub(1, Ordering::SeqCst);
        debug!("collector: command collector stop");
    });

    while !started.load(Ordering::SeqCst) {
        // busy waiting for the thread to start. (components_to_stop is added)
    }

    (rx_item, tx_interrupt)
}

type CommandOutput = (Option<Child>, Box<dyn BufRead + Send>);
fn get_command_output(cmd: &str) -> Result<CommandOutput, Box<dyn Error>> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let mut command: Child = Command::new(shell)
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = command
        .stdout
        .take()
        .ok_or_else(|| "command output: unwrap failed".to_owned())?;

    Ok((Some(command), Box::new(BufReader::new(stdout))))
}

//------------------------------------------------------------------------------
// helper
pub struct SkimItemReader {
    buf_size: usize,
    line_ending: u8,
}

impl Default for SkimItemReader {
    fn default() -> Self {
        Self {
            buf_size: ITEM_CHANNEL_SIZE,
            line_ending: b'\n',
        }
    }
}

impl SkimItemReader {
    pub fn buf_size(mut self, buf_size: usize) -> Self {
        self.buf_size = buf_size;
        self
    }

    pub fn line_ending(mut self, line_ending: u8) -> Self {
        self.line_ending = line_ending;
        self
    }
}

impl SkimItemReader {
    /// helper: convert bufread into SkimItemReceiver
    pub fn of_bufread(&self, mut source: impl BufRead + Send + 'static) -> SkimItemReceiver {
        let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) = bounded(self.buf_size);
        let line_ending = self.line_ending;
        thread::spawn(move || {
            let mut buffer = Vec::with_capacity(1024);
            loop {
                buffer.clear();
                // start reading
                match source.read_until(line_ending, &mut buffer) {
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

                        let string = String::from_utf8_lossy(&buffer);
                        let result = tx_item.send(Arc::new(string.into_owned()));
                        if result.is_err() {
                            break;
                        }
                    }
                    Err(_err) => {} // String not UTF8 or other error, skip.
                }
            }
        });
        rx_item
    }
}

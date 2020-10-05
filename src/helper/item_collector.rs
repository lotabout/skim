use crate::field::FieldRange;
use crate::helper::item::DefaultSkimItem;
use crate::reader::CommandCollector;
use crate::{SkimItem, SkimItemReceiver, SkimItemSender};
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
    use_ansi_color: bool,
    transform_fields: Vec<FieldRange>,
    matching_fields: Vec<FieldRange>,
    delimiter: Regex,
    replace_str: String,
    line_ending: u8,
}

impl Default for CollectorOption {
    fn default() -> Self {
        Self {
            use_ansi_color: false,
            transform_fields: Vec::new(),
            matching_fields: Vec::new(),
            delimiter: Regex::new(DELIMITER_STR).unwrap(),
            replace_str: "{}".to_string(),
            line_ending: b'\n',
        }
    }
}

impl CollectorOption {
    pub fn ansi(mut self, enable: bool) -> Self {
        self.use_ansi_color = enable;
        self
    }

    pub fn delimiter(mut self, delimiter: &str) -> Self {
        if !delimiter.is_empty() {
            self.delimiter = Regex::new(delimiter).unwrap_or_else(|_| Regex::new(DELIMITER_STR).unwrap());
        }
        self
    }

    pub fn with_nth(mut self, with_nth: &str) -> Self {
        if !with_nth.is_empty() {
            self.transform_fields = with_nth
                .split(',')
                .filter_map(|string| FieldRange::from_str(string))
                .collect();
        }
        self
    }

    pub fn transform_fields(mut self, transform_fields: Vec<FieldRange>) -> Self {
        self.transform_fields = transform_fields;
        self
    }

    pub fn nth(mut self, nth: &str) -> Self {
        if !nth.is_empty() {
            self.matching_fields = nth
                .split(',')
                .filter_map(|string| FieldRange::from_str(string))
                .collect();
        }
        self
    }

    pub fn matching_fields(mut self, matching_fields: Vec<FieldRange>) -> Self {
        self.matching_fields = matching_fields;
        self
    }

    pub fn replace_str(mut self, replace_str: &str) -> Self {
        if !replace_str.is_empty() {
            self.replace_str = replace_str.to_string();
        }
        self
    }

    pub fn read0(mut self, enable: bool) -> Self {
        if enable {
            self.line_ending = b'\0';
        } else {
            self.line_ending = b'\n';
        }
        self
    }

    pub fn build(self) -> Self {
        self
    }
}

pub enum CollectorInput {
    Pipe(Box<dyn BufRead + Send>),
    Command(String),
}

pub struct DefaultSkimCollector {
    option: Arc<CollectorOption>,
}

impl DefaultSkimCollector {
    pub fn new(option: CollectorOption) -> Self {
        Self {
            option: Arc::new(option),
        }
    }

    /// components_to_stop == 0 => all the threads have been stopped
    /// return (channel_for_receive_item, channel_to_stop_command)
    pub fn read_and_collect_from_command(
        &self,
        components_to_stop: Arc<AtomicUsize>,
        input: CollectorInput,
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
        let option = self.option.clone();
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

            let mut buffer = Vec::with_capacity(READ_BUFFER_SIZE);
            loop {
                buffer.clear();

                // start reading
                match source.read_until(option.line_ending, &mut buffer) {
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
                            option.use_ansi_color,
                            &option.transform_fields,
                            &option.matching_fields,
                            &option.delimiter,
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
}

impl CommandCollector for DefaultSkimCollector {
    fn invoke(&mut self, cmd: &str, components_to_stop: Arc<AtomicUsize>) -> (SkimItemReceiver, Sender<i32>) {
        self.read_and_collect_from_command(components_to_stop, CollectorInput::Command(cmd.to_string()))
    }
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

use std::fs::File;
use std::io::{BufRead, ErrorKind, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::process::{Child, Command, Stdio};
use std::{env, thread};
use std::error::Error;
use std::thread::JoinHandle;
use std::time::Duration;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::unistd::pipe;
use regex::Regex;
use crate::{Arc, ProviderSource, ReadAndAsRawFd, SkimItemPool, SkimItemProvider, SkimItemProviderControl};
use crate::field::FieldRange;
use crate::helper::item::DefaultSkimItem;
use crate::helper::sys_util::wait_until_ready;
use crate::helper::sys_util::WaitState::INTERRUPTED;


const DELIMITER_STR: &str = r"[\t\n ]+";
const READ_BUFFER_SIZE: usize = 1024;


pub struct DefaultSkimProviderOption {
    buf_size: usize,
    use_ansi_color: bool,
    transform_fields: Vec<FieldRange>,
    matching_fields: Vec<FieldRange>,
    delimiter: Regex,
    line_ending: u8,
    show_error: bool,
}

impl Default for DefaultSkimProviderOption {
    fn default() -> Self {
        Self {
            buf_size: READ_BUFFER_SIZE,
            line_ending: b'\n',
            use_ansi_color: false,
            transform_fields: Vec::new(),
            matching_fields: Vec::new(),
            delimiter: Regex::new(DELIMITER_STR).unwrap(),
            show_error: false,
        }
    }
}

impl DefaultSkimProviderOption {
    pub fn buf_size(mut self, buf_size: usize) -> Self {
        self.buf_size = buf_size;
        self
    }

    pub fn line_ending(mut self, line_ending: u8) -> Self {
        self.line_ending = line_ending;
        self
    }

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
            self.transform_fields = with_nth.split(',').filter_map(FieldRange::from_str).collect();
        }
        self
    }

    pub fn transform_fields(mut self, transform_fields: Vec<FieldRange>) -> Self {
        self.transform_fields = transform_fields;
        self
    }

    pub fn nth(mut self, nth: &str) -> Self {
        if !nth.is_empty() {
            self.matching_fields = nth.split(',').filter_map(FieldRange::from_str).collect();
        }
        self
    }

    pub fn matching_fields(mut self, matching_fields: Vec<FieldRange>) -> Self {
        self.matching_fields = matching_fields;
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

    pub fn show_error(mut self, show_error: bool) -> Self {
        self.show_error = show_error;
        self
    }

    pub fn build(self) -> Self {
        self
    }

    pub fn is_simple(&self) -> bool {
        !self.use_ansi_color && self.matching_fields.is_empty() && self.transform_fields.is_empty()
    }
}

pub struct DefaultSkimProviderControl {
    join_handle: Option<JoinHandle<()>>,
    // for joining
    command: Option<Child>,
    // for killing child process if exists
    wakeup_buf: File, // send anything to wake up the buf reading thread
}

impl SkimItemProviderControl for DefaultSkimProviderControl {
    fn kill_and_wait(&mut self) {
        // kill the child process if exists
        let _ = self.command.as_mut().map(|mut child| child.kill());
        // interrupt the buf reading thread
        let _ = self.wakeup_buf.write_all(b"x");
        let _ = self.wakeup_buf.flush();
        let _ = self.join_handle.take().map(|th| th.join());
    }

    fn is_done(&self) -> bool {
        self.join_handle.as_ref().map(|handle| handle.is_finished()).unwrap_or(true)
    }
}


pub struct DefaultSkimProvider {
    option: Arc<DefaultSkimProviderOption>,
}

impl DefaultSkimProvider {
    pub fn new(option: DefaultSkimProviderOption) -> Self {
        Self {
            option: Arc::new(option),
        }
    }

    pub fn option(mut self, option: DefaultSkimProviderOption) -> Self {
        self.option = Arc::new(option);
        self
    }
}

impl DefaultSkimProvider {
    fn generate_pipe() -> (File, File) {
        let (rx, tx) = pipe().expect("failed to set pipe");

        // set the signal pipe to non-blocking mode
        let flag = fcntl(rx, FcntlArg::F_GETFL).expect("Get fcntl failed");
        let mut flag = OFlag::from_bits_truncate(flag);
        flag.insert(OFlag::O_NONBLOCK);
        let _ = fcntl(rx, FcntlArg::F_SETFL(flag));

        unsafe { (File::from_raw_fd(rx), File::from_raw_fd(tx)) }
    }

    fn raw_bufread(&self, source: Box<dyn ReadAndAsRawFd>, item_pool: Arc<dyn SkimItemPool>) -> (File, JoinHandle<()>) {
        let line_ending = self.option.line_ending;
        let (rx_interrupt, tx_interrupt) = Self::generate_pipe();
        let option = self.option.clone();
        let join_handle = thread::spawn(move || {
            let mut buffer = Vec::with_capacity(1024);
            let mut bufreader = InterruptibleBufReader::new(source, Box::new(rx_interrupt));
            loop {
                buffer.clear();
                // start reading
                match bufreader.read_until(line_ending, &mut buffer) {
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

                        let buffer_taken = buffer;
                        buffer = Vec::with_capacity(1024);
                        let string = String::from_utf8(buffer_taken).expect("");
                        if option.is_simple() {
                            item_pool.push(Arc::new(string));
                        } else {
                            let raw_item = DefaultSkimItem::new(
                                string,
                                option.use_ansi_color,
                                &option.transform_fields,
                                &option.matching_fields,
                                &option.delimiter,
                            );
                            item_pool.push(Arc::new(raw_item));
                        }
                    }
                    Err(ref e) if e.kind() == ErrorKind::Interrupted=> break,
                    Err(_err) => {} // String not UTF8 or other error, skip.
                }
            }
        });
        (tx_interrupt, join_handle)
    }
}


impl SkimItemProvider for DefaultSkimProvider {
    fn run(&self, pool: Arc<dyn SkimItemPool>, source: ProviderSource) -> Box<dyn SkimItemProviderControl> {
        match source {
            ProviderSource::Pipe(pipe) => {
                let (tx_interrupt, join_handle) = self.raw_bufread(pipe, pool);
                Box::new(DefaultSkimProviderControl { join_handle: Some(join_handle), command: None, wakeup_buf: tx_interrupt })
            }
            ProviderSource::Command(cmd) => {
                let (command, pipe) = get_command_output(&cmd).expect("command not found");
                let (tx_interrupt, join_handle) = self.raw_bufread(pipe, pool);
                Box::new(DefaultSkimProviderControl { join_handle: Some(join_handle), command, wakeup_buf: tx_interrupt })
            }
        }
    }
}

type CommandOutput = (Option<Child>, Box<dyn ReadAndAsRawFd>);

fn get_command_output(cmd: &str) -> Result<CommandOutput, Box<dyn Error>> {
    let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    let mut command: Child = Command::new(shell)
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = command
        .stdout
        .take()
        .ok_or_else(|| "command output: unwrap failed".to_owned())?;

    Ok((Some(command), Box::new(stdout)))
}

struct InterruptibleBufReader {
    source: Box<dyn ReadAndAsRawFd>,
    tx_interrupt: Box<dyn ReadAndAsRawFd>,
    buf: [u8; 1024],
    filled: usize,
    pos: usize,
}

impl InterruptibleBufReader {
    fn new(source: Box<dyn ReadAndAsRawFd>, tx_interrupt: Box<dyn ReadAndAsRawFd>) -> Self {
        Self { source, tx_interrupt, buf: [0; 1024], filled: 0, pos: 0 }
    }

    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.pos >= self.filled {
            let state = wait_until_ready(self.source.as_raw_fd(), Some(self.tx_interrupt.as_raw_fd()), Duration::from_secs(0));
            if state == INTERRUPTED {
                return Err(std::io::Error::new(ErrorKind::Interrupted, "by user signal"));
            }

            self.pos = 0;
            self.filled = self.source.read(&mut self.buf)?;
        }

        Ok(&self.buf[self.pos..self.filled])
    }

    fn consume(&mut self, amt: usize) {
        self.pos += amt;
    }

    fn read_until(&mut self, delim: u8, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        let mut read = 0;
        loop {
            let (done, used) = {
                let available = self.fill_buf()?;
                match available.iter().position(|&c| c == delim) {
                    Some(i) => {
                        buf.extend_from_slice(&available[..=i]);
                        (true, i + 1)
                    }
                    None => {
                        buf.extend_from_slice(available);
                        (false, available.len())
                    }
                }
            };
            self.consume(used);
            read += used;
            if done || used == 0 {
                return Ok(read);
            }
        }
    }
}
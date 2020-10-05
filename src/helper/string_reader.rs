use crate::prelude::bounded;
use crate::{SkimItemReceiver, SkimItemSender};
/// helper for turn a BufRead into a skim stream
use std::io::BufRead;
use std::sync::Arc;
use std::thread;

const ITEM_CHANNEL_SIZE: usize = 10240;

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

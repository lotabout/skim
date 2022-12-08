/// helper for turn a BufRead into a skim stream
use std::io::BufRead;
use std::sync::Arc;

use crossbeam::channel::Sender;
use regex::Regex;

use crate::field::FieldRange;
use crate::helper::item_reader::READ_BUFFER_SIZE;
use crate::SkimItem;

use super::item::DefaultSkimItem;

#[derive(Clone)]
pub enum SendRawOrBuild<'a> {
    Raw,
    Build(BuildOptions<'a>),
}

#[derive(Clone)]
pub struct BuildOptions<'a> {
    pub ansi_enabled: bool,
    pub trans_fields: &'a [FieldRange],
    pub matching_fields: &'a [FieldRange],
    pub delimiter: &'a Regex,
}

#[allow(unused_assignments)]
pub fn ingest_loop(
    mut source: impl BufRead + Send + 'static,
    line_ending: u8,
    tx_item: Sender<Arc<dyn SkimItem>>,
    opts: SendRawOrBuild,
) {
    let mut bytes_buffer = Vec::with_capacity(READ_BUFFER_SIZE);

    loop {
        // first, read lots of bytes into the buffer
        bytes_buffer = if let Ok(res) = source.fill_buf() {
            res.to_vec()
        } else {
            break;
        };
        source.consume(bytes_buffer.len());

        // now, keep reading to make sure we haven't stopped in the middle of a word.
        // no need to add the bytes to the total buf_len, as these bytes are auto-"consumed()",
        // and bytes_buffer will be extended from slice to accommodate the new bytes
        if source.read_until(line_ending, &mut bytes_buffer).is_err() {
            break;
        };

        // break when there is nothing left to read
        if bytes_buffer.is_empty() {
            break;
        }

        if let Ok(unwrapped) = std::str::from_utf8_mut(&mut bytes_buffer) {
            let res = unwrapped
                .split(&['\n', line_ending as char])
                .map(|line| {
                    if line.ends_with("\r\n") {
                        line.trim_end_matches("\r\n")
                    } else if line.ends_with('\r') {
                        line.trim_end_matches('\r')
                    } else {
                        line
                    }
                })
                .map(|line| line.to_owned())
                .try_for_each(|line| match &opts {
                    SendRawOrBuild::Raw => tx_item.try_send(Arc::new(line)),
                    SendRawOrBuild::Build(opts) => {
                        let item = DefaultSkimItem::new(
                            line,
                            opts.ansi_enabled,
                            opts.trans_fields,
                            opts.matching_fields,
                            opts.delimiter,
                        );

                        tx_item.try_send(Arc::new(item))
                    }
                });
            if res.is_err() {
                break;
            }
        } else {
            break;
        }
    }
}

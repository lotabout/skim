#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
mod ansi;
mod casmutex;
mod curses;
mod event;
mod field;
mod input;
mod item;
mod matcher;
mod model;
mod options;
mod orderredvec;
mod output;
mod previewer;
mod query;
mod reader;
mod score;
mod sender;
mod theme;
mod util;

use curses::Curses;
use event::Event::*;
use event::{EventReceiver, EventSender};
use item::Item;
use nix::unistd::isatty;
pub use options::SkimOptions;
pub use output::SkimOutput;
use std::env;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::io::AsRawFd;
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tuikit::term::{Term, TermHeight, TermOptions};

const REFRESH_DURATION: u64 = 200;

pub struct Skim {}

impl Skim {
    pub fn run_with(options: &SkimOptions, source: Option<Box<BufRead + Send>>) -> Option<SkimOutput> {
        let min_height = options
            .min_height
            .map(Skim::parse_height_string)
            .expect("min_height should have default values");
        let height = options
            .height
            .map(Skim::parse_height_string)
            .expect("height should have default values");

        let term = Arc::new(Term::with_options(TermOptions::default().min_height(min_height).height(height)).unwrap());

        //------------------------------------------------------------------------------
        // curses

        // in piped situation(e.g. `echo "a" | sk`) set source to the pipe
        let source = source.or_else(|| {
            let stdin = std::io::stdin();
            if !isatty(stdin.as_raw_fd()).unwrap_or(true) {
                Some(Box::new(BufReader::new(stdin)))
            } else {
                None
            }
        });

        let curses = Curses::new(term.clone(), &options);

        //------------------------------------------------------------------------------
        // query
        let default_command = match env::var("SKIM_DEFAULT_COMMAND").as_ref().map(String::as_ref) {
            Ok("") | Err(_) => "find .".to_owned(),
            Ok(val) => val.to_owned(),
        };
        let mut query = query::Query::from_options(&options).base_cmd(&default_command).build();

        //------------------------------------------------------------------------------
        // reader -- read items from stdin or output of comment

        debug!("reader start");
        let (tx_reader, rx_reader) = channel();
        let (tx_item, rx_item) = sync_channel(128);
        let mut reader = reader::Reader::new(rx_reader, tx_item.clone(), source);
        reader.parse_options(&options);
        thread::spawn(move || {
            reader.run();
        });

        //------------------------------------------------------------------------------
        // input
        let (tx_input, rx_input): (EventSender, EventReceiver) = channel();
        let tx_input_clone = tx_input.clone();
        let mut input = input::Input::new(term.clone(), tx_input_clone);

        input.parse_keymaps(&options.bind);

        input.parse_expect_keys(options.expect.as_ref().map(|x| &**x));
        thread::spawn(move || {
            input.run();
        });

        //------------------------------------------------------------------------------
        // model + previewer
        let (tx_model, rx_model) = channel();
        let mut model = model::Model::new(rx_model);

        model.parse_options(&options);

        if options.preview.is_some() {
            let (tx_preview, rx_preview) = channel();
            model.set_previewer(tx_preview);
            // previewer
            let tx_model_clone = tx_model.clone();
            std::thread::spawn(move || {
                previewer::run(rx_preview, tx_model_clone);
            });
        }

        thread::spawn(move || {
            model.run(curses);
        });

        //------------------------------------------------------------------------------
        // matcher
        let tx_model_clone = tx_model.clone();
        let mut matcher = matcher::Matcher::new(tx_model_clone);
        matcher.parse_options(&options);

        thread::spawn(move || {
            matcher.run(rx_item);
        });

        //------------------------------------------------------------------------------
        // start a timer for notifying refresh
        let tx_model_clone = tx_model.clone();
        thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(REFRESH_DURATION));
            let _ = tx_model_clone.send((EvModelDrawInfo, Box::new(true)));
        });

        //------------------------------------------------------------------------------
        // Helper functions

        // light up the fire
        let _ = tx_reader.send((EvReaderRestart, Box::new((query.get_cmd(), query.get_query(), false))));

        let redraw_query = |query: &query::Query| {
            let _ = tx_model.send((EvModelDrawQuery, Box::new(query.get_print_func())));
        };

        let on_query_change = |query: &query::Query| {
            // restart the reader with new parameter
            // send redraw event
            redraw_query(query);
            let _ = tx_reader.send((EvReaderRestart, Box::new((query.get_cmd(), query.get_query(), false))));
        };

        let on_query_force_update = |query: &query::Query| {
            // restart the reader with new parameter
            let _ = tx_reader.send((EvReaderRestart, Box::new((query.get_cmd(), query.get_query(), true))));
            // send redraw event
            redraw_query(query);
        };

        //------------------------------------------------------------------------------
        // main loop, listen for user input
        // now we can use
        // tx_reader: send message to reader
        // tx_model:  send message to model
        // rx_input:  receive keystroke events

        let mut ret = None;

        let _ = tx_input.send((EvActRedraw, Box::new(true))); // trigger draw
        while let Ok((ev, arg)) = rx_input.recv() {
            debug!("main: got event {:?}", ev);
            match ev {
                EvActDeleteCharEOF => {
                    let _ = tx_input.send((EvActAbort, Box::new(true))); // trigger draw
                }

                EvActRotateMode => {
                    // tell the matcher to switch mode
                    let _ = tx_item.send((EvActRotateMode, Box::new(false)));
                    on_query_force_update(&query);
                }

                EvActAccept => {
                    // kill reader
                    let (tx, rx): (Sender<usize>, Receiver<usize>) = channel();
                    let _ = tx_reader.send((EvActAccept, Box::new(tx)));
                    let _ = rx.recv();

                    // sync with model to quit
                    let accept_key = *arg.downcast::<Option<String>>().unwrap_or_else(|_| Box::new(None));

                    let (tx, rx): (Sender<Vec<Arc<Item>>>, Receiver<Vec<Arc<Item>>>) = channel();
                    let _ = tx_model.send((EvActAccept, Box::new(tx)));
                    let selected = rx.recv().expect("receiving selected item failure on accept");

                    ret = Some(SkimOutput {
                        accept_key,
                        query: query.get_query(),
                        cmd: query.get_cmd_query(),
                        selected_items: selected,
                    });

                    break;
                }

                EvActClearScreen | EvActRedraw => {
                    let _ = tx_model.send((EvActRedraw, Box::new(query.get_print_func())));
                }

                EvActAbort => {
                    // kill reader
                    let (tx, rx): (Sender<usize>, Receiver<usize>) = channel();
                    let _ = tx_reader.send((EvActAbort, Box::new(tx)));
                    let _ = rx.recv();

                    // kill model
                    let (tx, rx): (Sender<bool>, Receiver<bool>) = channel();
                    let _ = tx_model.send((EvActAbort, Box::new(tx)));
                    let _ = rx.recv();
                    break;
                }

                EvActUp | EvActDown | EvActToggle | EvActToggleDown | EvActToggleUp | EvActToggleAll
                | EvActSelectAll | EvActDeselectAll | EvActPageDown | EvActPageUp | EvActScrollLeft
                | EvActScrollRight => {
                    let _ = tx_model.send((ev, arg));
                }

                EvActTogglePreview => {
                    let _ = tx_model.send((ev, arg));
                    let _ = tx_model.send((EvActRedraw, Box::new(query.get_print_func())));
                }

                _ => {}
            }
        }

        ret
    }

    // 10 -> TermHeight::Fixed(10)
    // 10% -> TermHeight::Percent(10)
    fn parse_height_string(string: &str) -> TermHeight {
        if string.ends_with('%') {
            TermHeight::Percent(string[0..string.len() - 1].parse().unwrap_or(100))
        } else {
            TermHeight::Fixed(string.parse().unwrap_or(0))
        }
    }
}

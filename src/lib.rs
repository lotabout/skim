extern crate env_logger;
extern crate nix;
extern crate regex;
extern crate termion;
extern crate unicode_width;
#[macro_use]
extern crate log;
extern crate clap;

#[macro_use]
extern crate lazy_static;
mod ansi;
mod curses;
mod event;
mod field;
mod input;
mod item;
mod matcher;
mod model;
mod options;
mod orderedvec;
mod output;
mod query;
mod reader;
mod score;
mod sender;

use curses::Curses;
use event::Event::*;
use event::{EventReceiver, EventSender};
use item::Item;
use nix::libc;
use nix::sys::signal::{pthread_sigmask, sigaction, SaFlags, SigAction, SigHandler, SigSet, SigmaskHow, Signal};
pub use options::SkimOptions;
pub use output::SkimOutput;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::os::unix::io::{FromRawFd, IntoRawFd};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const REFRESH_DURATION: u64 = 200;

pub struct Skim {}

extern "C" fn handle_sigwiwnch(_: i32) {}

impl Skim {
    pub fn run_with(options: &SkimOptions, source: Option<Box<BufRead + Send>>) -> Option<SkimOutput> {
        let (tx_input, rx_input): (EventSender, EventReceiver) = channel();
        //------------------------------------------------------------------------------
        // register terminal resize event, `pthread_sigmask` should be run before any thread.

        let mut sigset = SigSet::empty();
        sigset.add(Signal::SIGWINCH);
        let _ = pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&sigset), None);

        // SIGWINCH is ignored by mac by default, thus we need to register an empty handler
        let action = SigAction::new(SigHandler::Handler(handle_sigwiwnch), SaFlags::empty(), SigSet::empty());
        unsafe {
            let _ = sigaction(Signal::SIGWINCH, &action);
        }

        let tx_input_clone = tx_input.clone();
        thread::spawn(move || {
            // listen to the resize event;
            loop {
                let _errno = sigset.wait();
                if let Err(_) = tx_input_clone.send((EvActRedraw, Box::new(true))) {
                    break;
                }
            }
        });

        //------------------------------------------------------------------------------
        // curses

        // termion require the stdin to be terminal file
        // see: https://github.com/ticki/termion/issues/64
        // Here is a workaround. But reader will need to know the real stdin.
        let istty = unsafe { libc::isatty(libc::STDIN_FILENO as i32) } != 0;

        let source = source.or_else(|| {
            if !istty {
                unsafe {
                    let stdin = File::from_raw_fd(libc::dup(libc::STDIN_FILENO));
                    let tty = File::open("/dev/tty").expect("main: failed to open /dev/tty");
                    libc::dup2(tty.into_raw_fd(), libc::STDIN_FILENO);
                    Some(Box::new(BufReader::new(stdin)))
                }
            } else {
                None
            }
        });

        let curses = Curses::new(&options);

        //------------------------------------------------------------------------------
        // query
        let default_command = match env::var("SKIM_DEFAULT_COMMAND").as_ref().map(String::as_ref) {
            Ok("") | Err(_) => "find .".to_owned(),
            Ok(val) => val.to_owned(),
        };
        let mut query = query::Query::builder().base_cmd(&default_command).build();
        query.parse_options(&options);

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
        let tx_input_clone = tx_input.clone();
        let mut input = input::Input::new(tx_input_clone);

        input.parse_keymaps(&options.bind);

        input.parse_expect_keys(options.expect.as_ref().map(|x| &**x));
        thread::spawn(move || {
            input.run();
        });

        //------------------------------------------------------------------------------
        // model
        let (tx_model, rx_model) = channel();
        let mut model = model::Model::new(rx_model);

        model.parse_options(&options);
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
                EvActAddChar => {
                    let ch: char = *arg.downcast().expect("EvActAddChar: failed to get argument");
                    query.act_add_char(ch);
                    on_query_change(&query);
                }

                EvActBackwardDeleteChar => {
                    query.act_backward_delete_char();
                    on_query_change(&query);
                }

                EvActDeleteCharEOF | EvActDeleteChar => {
                    query.act_delete_char();
                    on_query_change(&query);
                }

                EvActBackwardChar => {
                    query.act_backward_char();
                    redraw_query(&query);
                }

                EvActForwardChar => {
                    query.act_forward_char();
                    redraw_query(&query);
                }

                EvActBackwardKillWord => {
                    query.act_backward_kill_word();
                    on_query_change(&query);
                }

                EvActUnixWordRubout => {
                    query.act_unix_word_rubout();
                    on_query_change(&query);
                }

                EvActBackwardWord => {
                    query.act_backward_word();
                    redraw_query(&query);
                }

                EvActForwardWord => {
                    query.act_forward_word();
                    redraw_query(&query);
                }

                EvActBeginningOfLine => {
                    query.act_beginning_of_line();
                    redraw_query(&query);
                }

                EvActEndOfLine => {
                    query.act_end_of_line();
                    redraw_query(&query);
                }

                EvActKillLine => {
                    query.act_kill_line();
                    on_query_change(&query);
                }

                EvActUnixLineDiscard => {
                    query.act_line_discard();
                    on_query_change(&query);
                }

                EvActKillWord => {
                    query.act_kill_word();
                    on_query_change(&query);
                }

                EvActYank => {
                    query.act_yank();
                    on_query_change(&query);
                }

                EvActToggleInteractive => {
                    query.act_query_toggle_interactive();
                    redraw_query(&query);
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

                EvReportCursorPos => {
                    let (y, x): (u16, u16) = *arg.downcast().expect("EvReportCursorPos: failed to get arguments");
                    debug!("main:EvReportCursorPos: {}/{}", y, x);
                }
                _ => {}
            }
        }

        ret
    }
}

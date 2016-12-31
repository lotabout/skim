#![feature(alloc_system)]
#![feature(io)]
extern crate alloc_system;
extern crate libc;
extern crate ncurses;
extern crate getopts;
extern crate regex;
#[macro_use] extern crate lazy_static;
mod item;
mod reader;
mod input;
mod matcher;
mod event;
mod model;
mod score;
mod orderedvec;
mod curses;
mod query;
mod ansi;
mod sender;

use std::thread;
use std::time::Duration;
use getopts::Options;
use std::env;
use std::sync::mpsc::{sync_channel, channel, Sender, Receiver};
use event::Event::*;

use std::io::Write;
macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

fn main() {
    let exit_code = real_main();
    std::process::exit(exit_code);
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

fn real_main() -> i32 {

    //------------------------------------------------------------------------------
    // parse options
    let mut opts = Options::new();
    opts.optopt("b", "bind", "comma seperated keybindings, such as 'ctrl-j:accept,ctrl-k:kill-line'", "KEY:ACTION");
    opts.optflag("h", "help", "print this help menu");
    opts.optflag("m", "multi", "Enable Multiple Selection");
    opts.optflag("", "no-multi", "Disable Multiple Selection");
    opts.optopt("p", "prompt", "prompt string", "'> '");
    opts.optopt("", "cmd-prompt", "prompt string", "'> '");
    opts.optopt("e", "expect", "comma seperated keys that can be used to complete skim", "KEYS");
    opts.optopt("t", "tiebreak", "comma seperated criteria", "[score,index,begin,end,-score,...]");
    opts.optflag("", "ansi", "parse ANSI color codes for input strings");
    opts.optflag("", "exact", "start skim in exact mode");
    opts.optopt("c", "cmd", "command to invoke dynamically", "ag");
    opts.optflag("i", "interactive", "Use skim as an interactive interface");
    opts.optopt("q", "query", "specify the initial query", "\"\"");
    opts.optflag("", "regex", "use regex instead of fuzzy match");
    opts.optopt("d", "delimiter", "specify the delimiter(in REGEX) for fields", "\\t");
    opts.optopt("n", "nth", "specify the fields to be matched", "1,2..5");
    opts.optopt("", "with-nth", "specify the fields to be transformed", "1,2..5");
    opts.optopt("I", "", "replace `replstr` with the selected item", "replstr");
    opts.optflag("", "version", "print out the current version of skim");

    let mut args = Vec::new();

    let program = env::args().nth(0).unwrap_or("sk".to_string());
    for arg in env::args().skip(1) {
        args.push(arg);
    }
    let options = match opts.parse(args) {
        Ok(m) => { m }
        Err(f) => { panic!(f.to_string()) }
    };

    // print help message
    if options.opt_present("h") {
        print_usage(&program, opts);
        return 0;
    }

    // print version
    if options.opt_present("version") {
        println!("0.1.2");
        return 0;
    }

    //------------------------------------------------------------------------------
    // query
    let default_command = match env::var("SKIM_DEFAULT_COMMAND") {
        Ok(val) => val,
        Err(_) => "find .".to_string(),
    };
    let mut query = query::Query::builder()
        .cmd(&default_command)
        .build();
    query.parse_options(&options);

    //------------------------------------------------------------------------------
    // reader
    let (tx_reader, rx_reader) = channel();
    let (tx_item, rx_item) = sync_channel(1024);
    let mut reader = reader::Reader::new(rx_reader, tx_item);
    reader.parse_options(&options);
    thread::spawn(move || {
        reader.run();
    });

    //------------------------------------------------------------------------------
    // matcher
    let (tx_model, rx_model) = channel();
    let tx_model_clone = tx_model.clone();
    let mut matcher = matcher::Matcher::new(rx_item, tx_model_clone);
    matcher.parse_options(&options);

    thread::spawn(move || {
        matcher.run();
    });

    //------------------------------------------------------------------------------
    // model
    let mut model = model::Model::new(rx_model);
    model.parse_options(&options);
    thread::spawn(move || {
        model.run();
    });

    //------------------------------------------------------------------------------
    // input
    let (tx_input, rx_input) = channel();
    let tx_input_clone = tx_input.clone();
    let mut input = input::Input::new(tx_input_clone);
    input.parse_keymap(options.opt_str("b"));
    input.parse_expect_keys(options.opt_str("e"));
    thread::spawn(move || {
        input.run();
    });

    //------------------------------------------------------------------------------
    // start a timer for notifying refresh
    let tx_input_clone = tx_input.clone();
    let (tx_view, rx_view) = channel();
    thread::spawn(move || {
        loop {
            let timeout = rx_view.recv_timeout(Duration::from_millis(150));
            if timeout.is_ok() {
                // to prevent from bounds, remove sequent urgent refresh
                thread::sleep(Duration::from_millis(50));
                while let Ok(_) = rx_view.try_recv() {}

                // some urgent refresh is needed
                let _ = tx_input_clone.send((EvActRedraw, Box::new(true)));
            } else {
                let _ = tx_input_clone.send((EvActRedraw, Box::new(true)));
            }
        }
    });

    //------------------------------------------------------------------------------
    // Helper functions

    // light up the fire
    let _ = tx_reader.send((EvReaderRestart, Box::new((query.get_cmd(), query.get_query()))));

    let on_query_change = |query: &query::Query| {
        // restart the reader with new parameter
        let _ = tx_reader.send((EvReaderRestart, Box::new((query.get_cmd(), query.get_query()))));
        // send redraw event
        let _ = tx_view.send(true);
    };

    let redraw_screen = || { let _ = tx_view.send(true); };

    //------------------------------------------------------------------------------
    // main loop, listen for user input
    // now we can use
    // tx_reader: send message to reader
    // tx_model:  send message to model
    // rx_input:  receive keystroke events

    let mut exit_code = 1;
    while let Ok((ev, arg)) = rx_input.recv() {
        match ev {
            EvActAddChar =>  {
                let ch: char = *arg.downcast().unwrap();
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
                redraw_screen();
            }

            EvActForwardChar => {
                query.act_forward_char();
                redraw_screen();
            }

            EvActBackwardKillWord | EvActUnixWordRubout => {
                query.act_backward_kill_word();
                on_query_change(&query);
            }

            EvActBackwardWord => {
                query.act_backward_word();
                redraw_screen();
            }

            EvActForwardWord => {
                query.act_forward_word();
                redraw_screen();
            }

            EvActBeginningOfLine => {
                query.act_beginning_of_line();
                redraw_screen();
            }

            EvActEndOfLine => {
                query.act_end_of_line();
                redraw_screen();
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

            EvActRotateMode => {
                query.act_query_rotate_mode();
                redraw_screen();
            }

            EvActAccept => {
                // sync with model to quit

                let accept_key = *arg.downcast::<Option<String>>()
                    .unwrap_or(Box::new(None));

                let (tx, rx): (Sender<usize>, Receiver<usize>) = channel();
                let _ = tx_model.send((EvActAccept, Box::new((accept_key, tx))));
                let selected = rx.recv().unwrap_or(0);;
                exit_code = if selected > 0 {0} else {1};
                break;
            }

            EvActClearScreen | EvActRedraw => {
                let _ = tx_model.send((EvModelRedraw, Box::new(query.get_print_func())));
            }

            EvActAbort => {
                exit_code = 130;
                break;
            }

            EvActUp | EvActDown
                | EvActToggle | EvActToggleDown | EvActToggleUp
                | EvActToggleAll | EvActSelectAll | EvActDeselectAll
                | EvActPageDown | EvActPageUp
                | EvActScrollLeft | EvActScrollRight => {
                let _ = tx_model.send((ev, arg));
                redraw_screen();
            }

            _ => {}
        }
    }

    exit_code
}

#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
extern crate libc;
extern crate getopts;
extern crate regex;
extern crate shlex;
extern crate utf8parse;
extern crate unicode_width;
extern crate termion;
#[macro_use] extern crate log;
extern crate env_logger;
extern crate time;

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
use event::{Event, EventArg};
use std::mem;
use std::ptr;
use libc::{sigemptyset, sigaddset, sigwait, pthread_sigmask};
use curses::Curses;
use std::fs::File;
use std::os::unix::io::{FromRawFd, IntoRawFd};

const REFRESH_DURATION: u64 = 200;

const USAGE : &'static str = "
Usage: sk [options]

  Options
    -h, --help           print this help menu
    --version            print out the current version of skim

  Search
    -t, --tiebreak [score,index,begin,end,-score,...]
                         comma seperated criteria
    -n, --nth 1,2..5     specify the fields to be matched
    --with-nth 1,2..5    specify the fields to be transformed
    -d, --delimiter \\t  specify the delimiter(in REGEX) for fields
    --exact              start skim in exact mode
    --regex              use regex instead of fuzzy match

  Interface
    -b, --bind KEYBINDS  comma seperated keybindings, in KEY:ACTION
                         such as 'ctrl-j:accept,ctrl-k:kill-line'
    -m, --multi          Enable Multiple Selection
    --no-multi           Disable Multiple Selection
    -p, --prompt '> '    prompt string for query mode
    --cmd-prompt '> '    prompt string for command mode
    -c, --cmd ag         command to invoke dynamically
    -I replstr           replace `replstr` with the selected item
    -i, --interactive    Start skim in interactive(command) mode
    --ansi               parse ANSI color codes for input strings
    --color [BASE][,COLOR:ANSI]
                         change color theme
    --reverse            Reverse orientation
    --height=HEIGHT      Height of skim's window (--height 40%)
    --margin=MARGIN      Screen Margin (TRBL / TB,RL / T,RL,B / T,R,B,L)
                         e.g. (sk --margin 1,10%)

  Scripting
    -q, --query \"\"       specify the initial query
    -e, --expect KEYS    comma seperated keys that can be used to complete skim

  Environment variables
    SKIM_DEFAULT_COMMAND Default command to use when input is tty
    SKIM_DEFAULT_OPTIONS Default options (e.g. '--ansi --regex')
                         You should not include other environment variables
                         (e.g. '-c \"$HOME/bin/ag\"')
";

fn main() {
    use log::{LogRecord, LogLevelFilter};
    use env_logger::LogBuilder;

    let format = |record: &LogRecord| {
        let t = time::now();
        format!("{},{:03} - {} - {}",
                time::strftime("%Y-%m-%d %H:%M:%S", &t).unwrap(),
                t.tm_nsec / 1000_000,
                record.level(),
                record.args()
               )
    };

    let mut builder = LogBuilder::new();
    builder.format(format).filter(None, LogLevelFilter::Info);

    if env::var("RUST_LOG").is_ok() {
        builder.parse(&env::var("RUST_LOG").unwrap());
    }

    builder.init().unwrap();

    let exit_code = real_main();
    std::process::exit(exit_code);
}

fn print_usage() {
    print!("{}", USAGE);

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
    opts.optopt("", "color", "change color theme", "[BASE][,COLOR:ANSI]");
    opts.optopt("", "margin", "margin around the finder", "");
    opts.optopt("", "height", "height", "");
    opts.optflag("", "reverse", "reverse orientation");
    opts.optflag("", "version", "print out the current version of skim");

    let mut args = Vec::new();

    args.extend(env::var("SKIM_DEFAULT_OPTIONS")
                .ok()
                .and_then(|val| shlex::split(&val))
                .unwrap_or(Vec::new()));

    for arg in env::args().skip(1) {
        args.push(arg);
    }

    let options = match opts.parse(args) {
        Ok(m) => { m }
        Err(f) => { panic!(f.to_string()) }
    };

    // print help message
    if options.opt_present("h") {
        print_usage();
        return 0;
    }

    // print version
    if options.opt_present("version") {
        println!("0.2.1-beta.2");
        return 0;
    }

    let (tx_input, rx_input): (Sender<(Event, EventArg)>, Receiver<(Event, EventArg)>) = channel();
    //------------------------------------------------------------------------------
    // register terminal resize event, `pthread_sigmask` should be run before any thread.
    let mut sigset = unsafe {mem::uninitialized()};
    unsafe {
        sigemptyset(&mut sigset);
        sigaddset(&mut sigset, libc::SIGWINCH);
        pthread_sigmask(libc::SIG_SETMASK, &sigset, ptr::null_mut());
    }

    let tx_input_clone = tx_input.clone();
    thread::spawn(move || {
        // listen to the resize event;
        loop {
            let mut sig = 0;
            let _errno = unsafe {sigwait(&sigset, &mut sig)};
            let _ = tx_input_clone.send((EvActRedraw, Box::new(true)));
        }
    });

    //------------------------------------------------------------------------------
    // curses

    // termion require the stdin to be terminal file
    // see: https://github.com/ticki/termion/issues/64
    // Here is a workaround. But reader will need to know the real stdin.
    let istty = unsafe { libc::isatty(libc::STDIN_FILENO as i32) } != 0;
    let real_stdin = if !istty {
        unsafe {
            let stdin = File::from_raw_fd(libc::dup(libc::STDIN_FILENO));
            let tty = File::open("/dev/tty").unwrap();
            libc::dup2(tty.into_raw_fd(), libc::STDIN_FILENO);
            Some(stdin)
        }
    } else {
        None
    };

    let curses = Curses::new(&options);

    //------------------------------------------------------------------------------
    // query
    let default_command = match env::var("SKIM_DEFAULT_COMMAND") {
        Ok(val) => val,
        Err(_) => "find .".to_string(),
    };
    let mut query = query::Query::builder()
        .base_cmd(&default_command)
        .build();
    query.parse_options(&options);

    //------------------------------------------------------------------------------
    // reader -- read items from stdin or output of comment

    debug!("reader start");
    let (tx_reader, rx_reader) = channel();
    let (tx_item, rx_item) = sync_channel(128);
    let mut reader = reader::Reader::new(rx_reader, tx_item.clone(), real_stdin);
    reader.parse_options(&options);
    thread::spawn(move || {
        reader.run();
    });

    //------------------------------------------------------------------------------
    // input
    let tx_input_clone = tx_input.clone();
    let mut input = input::Input::new(tx_input_clone);
    input.parse_keymap(options.opt_str("b"));
    input.parse_expect_keys(options.opt_str("e"));
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
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(REFRESH_DURATION));
            let _ = tx_model_clone.send((EvModelDrawInfo, Box::new(true)));
        }
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

    let mut exit_code = 1;

    let _ = tx_input.send((EvActRedraw, Box::new(true))); // trigger draw
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
                redraw_query(&query);
            }

            EvActForwardChar => {
                query.act_forward_char();
                redraw_query(&query);
            }

            EvActBackwardKillWord | EvActUnixWordRubout => {
                query.act_backward_kill_word();
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

                let accept_key = *arg.downcast::<Option<String>>()
                    .unwrap_or(Box::new(None));

                let (tx, rx): (Sender<usize>, Receiver<usize>) = channel();
                let _ = tx_model.send((EvActAccept, Box::new((accept_key, tx))));
                let selected = rx.recv().unwrap_or(0);
                exit_code = if selected > 0 {0} else {1};
                break;
            }

            EvActClearScreen | EvActRedraw => {
                let _ = tx_model.send((EvActRedraw, Box::new(query.get_print_func())));
            }

            EvActAbort => {
                let (tx, rx): (Sender<bool>, Receiver<bool>) = channel();
                let _ = tx_model.send((EvActAbort, Box::new(tx)));
                let _ = rx.recv();
                exit_code = 130;
                break;
            }

            EvActUp | EvActDown
                | EvActToggle | EvActToggleDown | EvActToggleUp
                | EvActToggleAll | EvActSelectAll | EvActDeselectAll
                | EvActPageDown | EvActPageUp
                | EvActScrollLeft | EvActScrollRight => {
                let _ = tx_model.send((ev, arg));
            }

            EvReportCursorPos => {
                let (y, x): (u16, u16) = *arg.downcast().unwrap();
                debug!("main:EvReportCursorPos: {}/{}", y, x);
            }
            _ => {}
        }
    }

    exit_code
}

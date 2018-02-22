extern crate clap;
extern crate env_logger;
extern crate libc;
#[macro_use]
extern crate log;
extern crate regex;
extern crate shlex;
extern crate termion;
extern crate time;
extern crate unicode_width;
extern crate utf8parse;

#[macro_use]
extern crate lazy_static;
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
mod field;

use std::thread;
use std::time::Duration;
use std::env;
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender};
use event::Event::*;
use event::{EventReceiver, EventSender};
use std::mem;
use std::ptr;
use libc::{pthread_sigmask, sigaddset, sigemptyset, sigwait};
use curses::Curses;
use std::fs::File;
use std::os::unix::io::{FromRawFd, IntoRawFd};
use clap::{App, Arg};

const VERSION: &str = "0.3.2";

const USAGE: &str = "
Usage: sk [options]

  Options
    -h, --help           print this help menu
    --version            print out the current version of skim

  Search
    --tac                reverse the order of input
    -t, --tiebreak [score,index,begin,end,-score,...]
                         comma seperated criteria
    -n, --nth 1,2..5     specify the fields to be matched
    --with-nth 1,2..5    specify the fields to be transformed
    -d, --delimiter \\t  specify the delimiter(in REGEX) for fields
    -e, --exact          start skim in exact mode
    --regex              use regex instead of fuzzy match

  Interface
    -b, --bind KEYBINDS  comma seperated keybindings, in KEY:ACTION
                         such as 'ctrl-j:accept,ctrl-k:kill-line'
    -m, --multi          Enable Multiple Selection
    --no-multi           Disable Multiple Selection
    -c, --cmd ag         command to invoke dynamically
    -I replstr           replace `replstr` with the selected item
    -i, --interactive    Start skim in interactive(command) mode
    --color [BASE][,COLOR:ANSI]
                         change color theme
    --no-hscroll         Disable horizontal scroll

  Layout
    --reverse            Reverse orientation
    --height=HEIGHT      Height of skim's window (--height 40%)
    --no-height          Disable height feature
    --min-height=HEIGHT  Minimum height when --height is given by percent
                         (default: 10)
    --margin=MARGIN      Screen Margin (TRBL / TB,RL / T,RL,B / T,R,B,L)
                         e.g. (sk --margin 1,10%)
    -p, --prompt '> '    prompt string for query mode
    --cmd-prompt '> '    prompt string for command mode

  Display
    --ansi               parse ANSI color codes for input strings
    --tabstop=SPACES     Number of spaces for a tab character (default: 8)

  Preview
    --preview=COMMAND    command to preview current highlighted line ({})
                         We can specify the fields. e.g. ({1}, {..3}, {0..})
    --preview-window=OPT Preview window layout (default: right:50%)
                         [up|down|left|right][:SIZE[%]][:hidden]

  Scripting
    -q, --query \"\"       specify the initial query
    --cmd-query \"\"       specify the initial query for interactive mode
    --expect KEYS        comma seperated keys that can be used to complete skim
    --read0              Read input delimited by ASCII NUL(\\0) characters
    --print0             Print output delimited by ASCII NUL(\\0) characters
    --print-query        Print query as the first line
    --print-cmd          Print command query as the first line (after --print-query)

  Environment variables
    SKIM_DEFAULT_COMMAND Default command to use when input is tty
    SKIM_DEFAULT_OPTIONS Default options (e.g. '--ansi --regex')
                         You should not include other environment variables
                         (e.g. '-c \"$HOME/bin/ag\"')

  Reserved (not used for now)
    --extended
    --algo=TYPE
    --literal
    --no-mouse
    --cycle
    --hscroll-off=COL
    --filepath-word
    --jump-labels=CHARS
    --border
    --inline-info
    --header=STR
    --header-lines=N
    --no-bold
    --history=FILE
    --history-size=N
    --sync
    --no-sort
    --select-1
    --exit-0
    --filter
";

const REFRESH_DURATION: u64 = 200;

fn main() {
    use log::{LogLevelFilter, LogRecord};
    use env_logger::LogBuilder;

    let format = |record: &LogRecord| {
        let t = time::now();
        format!(
            "{},{:03} - {} - {}",
            time::strftime("%Y-%m-%d %H:%M:%S", &t).unwrap(),
            t.tm_nsec / 1_000_000,
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

#[cfg_attr(rustfmt, rustfmt_skip)]
fn real_main() -> i32 {
    let mut args = Vec::new();

    args.push(env::args().next().unwrap());
    args.extend(env::var("SKIM_DEFAULT_OPTIONS")
                .ok()
                .and_then(|val| shlex::split(&val))
                .unwrap_or_default());
    for arg in env::args().skip(1) {
        args.push(arg);
    }


    //------------------------------------------------------------------------------
    // parse options
    let options = App::new("sk")
        .author("Jinzhou Zhang<lotabout@gmail.com")
        .arg(Arg::with_name("help").long("help").short("h"))
        .arg(Arg::with_name("version").long("version").short("v"))
        .arg(Arg::with_name("bind").long("bind").short("b").multiple(true).takes_value(true))
        .arg(Arg::with_name("multi").long("multi").short("m").multiple(true))
        .arg(Arg::with_name("no-multi").long("no-multi").multiple(true))
        .arg(Arg::with_name("prompt").long("prompt").short("p").multiple(true).takes_value(true).default_value("> "))
        .arg(Arg::with_name("cmd-prompt").long("cmd-prompt").multiple(true).takes_value(true).default_value("c> "))
        .arg(Arg::with_name("expect").long("expect").multiple(true).takes_value(true))
        .arg(Arg::with_name("tac").long("tac").multiple(true))
        .arg(Arg::with_name("tiebreak").long("tiebreak").short("t").multiple(true).takes_value(true))
        .arg(Arg::with_name("ansi").long("ansi").multiple(true))
        .arg(Arg::with_name("exact").long("exact").short("e").multiple(true))
        .arg(Arg::with_name("cmd").long("cmd").short("cmd").multiple(true).takes_value(true))
        .arg(Arg::with_name("interactive").long("interactive").short("i").multiple(true))
        .arg(Arg::with_name("query").long("query").short("q").multiple(true).takes_value(true))
        .arg(Arg::with_name("cmd-query").long("cmd-query").multiple(true).takes_value(true))
        .arg(Arg::with_name("regex").long("regex").multiple(true))
        .arg(Arg::with_name("delimiter").long("delimiter").short("d").multiple(true).takes_value(true))
        .arg(Arg::with_name("nth").long("nth").short("n").multiple(true).takes_value(true))
        .arg(Arg::with_name("with-nth").long("with-nth").multiple(true).takes_value(true))
        .arg(Arg::with_name("replstr").short("I").multiple(true).takes_value(true))
        .arg(Arg::with_name("color").long("color").multiple(true).takes_value(true))
        .arg(Arg::with_name("margin").long("margin").multiple(true).takes_value(true).default_value("0,0,0,0"))
        .arg(Arg::with_name("min-height").long("min-height").multiple(true).takes_value(true).default_value("10"))
        .arg(Arg::with_name("height").long("height").multiple(true).takes_value(true).default_value("100%"))
        .arg(Arg::with_name("no-height").long("no-height").multiple(true))
        .arg(Arg::with_name("preview").long("preview").multiple(true).takes_value(true))
        .arg(Arg::with_name("preview-window").long("preview-window").multiple(true).takes_value(true).default_value("right:50%"))
        .arg(Arg::with_name("reverse").long("reverse").multiple(true))

        .arg(Arg::with_name("algorithm").long("algo").multiple(true).takes_value(true).default_value(""))
        .arg(Arg::with_name("literal").long("literal").multiple(true))
        .arg(Arg::with_name("no-mouse").long("no-mouse").multiple(true))
        .arg(Arg::with_name("cycle").long("cycle").multiple(true))
        .arg(Arg::with_name("no-hscroll").long("no-hscroll").multiple(true))
        .arg(Arg::with_name("hscroll-off").long("hscroll-off").multiple(true).takes_value(true).default_value("10"))
        .arg(Arg::with_name("filepath-word").long("filepath-word").multiple(true))
        .arg(Arg::with_name("jump-labels").long("jump-labels").multiple(true).takes_value(true).default_value("abcdefghijklmnopqrstuvwxyz"))
        .arg(Arg::with_name("border").long("border").multiple(true))
        .arg(Arg::with_name("inline-info").long("inline-info").multiple(true))
        .arg(Arg::with_name("header").long("header").multiple(true).takes_value(true).default_value(""))
        .arg(Arg::with_name("header-lines").long("header-lines").multiple(true).takes_value(true).default_value("1"))
        .arg(Arg::with_name("tabstop").long("tabstop").multiple(true).takes_value(true).default_value("8"))
        .arg(Arg::with_name("no-bold").long("no-bold").multiple(true))
        .arg(Arg::with_name("history").long("history").multiple(true).takes_value(true).default_value(""))
        .arg(Arg::with_name("history-size").long("history-size").multiple(true).takes_value(true).default_value("500"))
        .arg(Arg::with_name("print-query").long("print-query").multiple(true))
        .arg(Arg::with_name("print-cmd").long("print-cmd").multiple(true))
        .arg(Arg::with_name("read0").long("read0").multiple(true))
        .arg(Arg::with_name("print0").long("print0").multiple(true))
        .arg(Arg::with_name("sync").long("sync").multiple(true))
        .arg(Arg::with_name("extended").long("extended").short("x").multiple(true))
        .arg(Arg::with_name("no-sort").long("no-sort").multiple(true))
        .arg(Arg::with_name("select-1").long("select-1").short("1").multiple(true))
        .arg(Arg::with_name("exit-0").long("exit-0").short("0").multiple(true))
        .arg(Arg::with_name("filter").long("filter").short("f").multiple(true))
        .get_matches_from(args);

    if options.is_present("help") {
        print!("{}", USAGE);
        return 0;
    }

    if options.is_present("version") {
        println!("{}", VERSION);
        return 0;
    }

    let (tx_input, rx_input): (EventSender, EventReceiver) = channel();
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
    let default_command = match env::var("SKIM_DEFAULT_COMMAND").as_ref().map(String::as_ref) {
        Ok("") | Err(_) => "find .".to_owned(),
        Ok(val) => val.to_owned(),
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

    let keymaps = options.values_of("bind").map(|x| x.collect::<Vec<_>>()).unwrap_or_default();
    input.parse_keymaps(&keymaps);

    let expect_keys = options.values_of("expect").map(|x| x.collect::<Vec<_>>().join(","));
    input.parse_expect_keys(expect_keys.as_ref().map(|x| &**x));
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
        debug!("main: got event {:?}", ev);
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

            EvActYank=> {
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

                let accept_key = *arg.downcast::<Option<String>>()
                    .unwrap_or_else(|_| Box::new(None));

                let (tx, rx): (Sender<usize>, Receiver<usize>) = channel();
                let _ = tx_model.send((EvActAccept, Box::new((accept_key, query.get_query(), query.get_cmd_query(), tx))));
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

            EvActTogglePreview => {
                let _ = tx_model.send((ev, arg));
                let _ = tx_model.send((EvActRedraw, Box::new(query.get_print_func())));
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

#![feature(io)]
extern crate libc;
extern crate ncurses;
extern crate getopts;
extern crate regex;
#[macro_use] extern crate lazy_static;
mod util;
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

use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use std::mem;
use std::ptr;
use util::eventbox::EventBox;

use ncurses::*;
use event::Event::*;
use input::Input;
use reader::Reader;
use matcher::Matcher;
use model::Model;
use libc::{sigemptyset, sigaddset, sigwait, pthread_sigmask};
use curses::{ColorTheme, Curses};
use getopts::Options;
use std::env;
use orderedvec::OrderedVec;
use item::{Item, MatchedItem};

fn main() {
    let exit_code = real_main();
    std::process::exit(exit_code);
}

//fn real_main_old() -> i32 {

    //// option parsing

    //let mut opts = Options::new();
    //opts.optopt("b", "bind", "comma seperated keybindings, such as 'ctrl-j:accept,ctrl-k:kill-line'", "KEY:ACTION");
    //opts.optflag("h", "help", "print this help menu");
    //opts.optflag("m", "multi", "Enable Multiple Selection");
    //opts.optflag("", "no-multi", "Disable Multiple Selection");
    //opts.optopt("p", "prompt", "prompt string", "'> '");
    //opts.optopt("e", "expect", "comma seperated keys that can be used to complete fzf", "KEYS");
    //opts.optopt("t", "tiebreak", "comma seperated criteria", "[score,index,begin,end,-score,...]");
    //opts.optflag("", "ansi", "parse ANSI color codes for input strings");
    //opts.optopt("c", "cmd", "command to invoke dynamically", "ag");
    //opts.optflag("i", "interactive", "Use skim as an interactive interface");
    //opts.optflag("", "regex", "use regex instead of fuzzy match");
    //opts.optopt("q", "query", "specify the initial query", "\"\"");
    //opts.optopt("d", "delimiter", "specify the delimiter(in REGEX) for fields", "\\t");
    //opts.optopt("n", "nth", "specify the fields to be matched", "1,2..5");
    //opts.optopt("", "with-nth", "specify the fields to be transformed", "1,2..5");
    //opts.optopt("I", "", "replace `replstr` with the selected item", "replstr");
    //opts.optflag("", "version", "print out the current version of skim");

    //let default_options = match env::var("SKIM_DEFAULT_OPTIONS") {
        //Ok(val) => val,
        //Err(_) => "".to_string(),
    //};

    //let mut args = Vec::new();
    //for option in default_options.split(' ') {
        //args.push(option.to_string());
    //}

    //let program = env::args().nth(0).unwrap_or("sk".to_string());
    //for arg in env::args().skip(1) {
        //args.push(arg);
    //}

    //let options = match opts.parse(args) {
        //Ok(m) => { m }
        //Err(f) => { panic!(f.to_string()) }
    //};

    //// print help message
    //if options.opt_present("h") {
        //print_usage(&program, opts);
        //return 0;
    //}

    //// print version
    //if options.opt_present("version") {
        //println!("0.1.2");
        //return 0;
    //}

    //let theme = ColorTheme::new();
    //let curses = Curses::new();
    //curses::init(Some(&theme), false, false);


    //// register terminal resize event, `pthread_sigmask` should be run before any thread.
    //let mut sigset = unsafe {mem::uninitialized()};
    //unsafe {
        //sigemptyset(&mut sigset);
        //sigaddset(&mut sigset, libc::SIGWINCH);
        //pthread_sigmask(libc::SIG_SETMASK, &sigset, ptr::null_mut());
    //}

    //// controller variables
    //let eb = Arc::new(EventBox::new());
    //let item_buffer = Arc::new(RwLock::new(Vec::new()));

    //let eb_clone = eb.clone();
    //thread::spawn(move || {
        //// listen to the resize event;
        //loop {
            //let mut sig = 0;
            //let _errno = unsafe {sigwait(&sigset, &mut sig)};
            //eb_clone.set(Event::EvResize, Box::new(true));
        //}
    //});

    //// model
    //let mut model = Model::new(eb.clone(), curses);
    //model.parse_options(&options);


    //// matcher
    //let items = model.items.clone();
    //let mut matcher = Matcher::new(items, item_buffer.clone(), eb.clone());
    //let eb_matcher = matcher.eb_req.clone();
    //matcher.parse_options(&options);

    //// reader
    //let default_command = match env::var("SKIM_DEFAULT_COMMAND") {
        //Ok(val) => val,
        //Err(_) => "find .".to_string(),
    //};
    //let mut reader = Reader::new(default_command, eb.clone(), item_buffer.clone());
    //reader.parse_options(&options);
    //let eb_reader = reader.eb_req.clone();

    //// input
    //let mut input = Input::new(eb.clone());
    //input.parse_keymap(options.opt_str("b"));
    //input.parse_expect_keys(options.opt_str("e"));

    //// start running
    //thread::spawn(move || {
        //reader.run();
    //});

    //thread::spawn(move || {
        //matcher.run();
    //});

    //thread::spawn(move || {
        //input.run();
    //});

    //model.print_query(); // to place cursor
    //model.refresh();

    //let exit_code;

    //'outer: loop {
        //let mut immediate_refresh = false;
        //for (e, val) in eb.wait() {
            //match e {
                //Event::EvReaderNewItem => {
                    //eb_matcher.set(Event::EvMatcherNewItem, Box::new(true));
                    //let reading: bool = *val.downcast().unwrap();
                    //model.reading = reading;

                    //let new_items = item_buffer.read().unwrap();
                    //model.update_num_total(new_items.len());
                    //model.print_info();
                //}

                //Event::EvMatcherUpdateProcess => {
                    //let percentage: u64 = *val.downcast().unwrap();
                    //model.update_percentage(percentage);
                    //model.print_info();
                //}

                //Event::EvMatcherEnd => {
                    //// do nothing
                    //let result: Arc<RwLock<OrderedVec<MatchedItem>>> = *val.downcast().unwrap();
                    //model.update_percentage(100);
                    //model.update_matched_items(result);
                    //model.display();
                //}

                //Event::EvQueryChange => {
                    //let query: String = *val.downcast().unwrap();

                    //if model.is_interactive {
                        //model.clear_items();
                        //eb_matcher.set(Event::EvMatcherResetQuery, Box::new(query.clone()));
                        //eb.wait_for(Event::EvMatcherSync);
                        //eb_reader.set(Event::EvReaderResetQuery, Box::new(query.clone()));
                        //eb.wait_for(Event::EvReaderSync);
                        //eb_reader.set(Event::EvModelAck, Box::new(true));
                        //eb_matcher.set(Event::EvModelAck, Box::new(true));
                        //eb.clear();
                        //break;
                    //} else {
                        //eb_matcher.set(Event::EvMatcherResetQuery, Box::new(query));
                    //}
                //}

                //Event::EvInputInvalid => {
                    //// ignore
                //}

                //Event::EvInputKey => {
                    //// ignore for now
                //}

                //Event::EvResize => {
                    //model.resize();
                    //model.display();
                    //immediate_refresh = true;
                //}

                //Event::EvActAddChar => {
                    //let ch: char = *val.downcast().unwrap();
                    //model.act_add_char(ch);
                    //model.print_query();
                //}

                //// Actions
                //Event::EvActAbort => {
                    //exit_code = 130;
                    //break 'outer;
                //}

                //Event::EvActAccept => {
                    //// break out of the loop and output the selected item.
                    ////if model.get_num_selected() <= 0 { model.act_toggle(Some(true)); }
                    //let args: Option<String> = *val.downcast().unwrap();
                    //let num_selected = model.act_accept(args);
                    //exit_code = if num_selected > 0 {0} else {1};
                    //break 'outer;
                //}

                //Event::EvActBackwardChar => {
                    //model.act_backward_char();
                    //model.print_query();
                //}

                //Event::EvActBackwardDeleteChar => {
                    //model.act_backward_delete_char();
                    //model.print_query();
                //}

                //Event::EvActBackwardKillWord => {
                    //model.act_backward_kill_word();
                    //model.print_query();
                //}

                //Event::EvActBackwardWord => {
                    //model.act_backward_word();
                    //model.print_query();
                //}

                //Event::EvActBeginningOfLine => {
                    //model.act_beginning_of_line();
                    //model.print_query();
                //}

                //Event::EvActCancel => {}

                //Event::EvActClearScreen => {
                    //model.display();
                    //immediate_refresh = true;
                //}

                //Event::EvActDeleteChar => {
                    //model.act_delete_char();
                    //model.print_query();
                //}

                //Event::EvActDeleteCharEOF => {
                    //model.act_delete_char();
                    //model.print_query();
                //}

                //Event::EvActDeselectAll => {
                    //model.act_deselect_all();
                    //model.print_info();
                    //model.print_items();
                //}

                //Event::EvActDown => {
                    //model.act_move_line_cursor(-1);
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActEndOfLine => {
                    //model.act_end_of_line();
                    //model.print_query();
                //}

                //Event::EvActForwardChar => {
                    //model.act_forward_char();
                    //model.print_query();
                //}

                //Event::EvActForwardWord => {
                    //model.act_forward_word();
                    //model.print_query();
                //}

                //Event::EvActIgnore => {}

                //Event::EvActKillLine => {
                    //model.act_kill_line();
                    //model.print_query();
                //}

                //Event::EvActKillWord => {
                    //model.act_kill_word();
                    //model.print_query();
                //}

                //Event::EvActNextHistory => {}

                //Event::EvActPageDown => {
                    //model.act_move_page(-1);
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActPageUp => {
                    //model.act_move_page(1);
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActPreviousHistory => {}

                //Event::EvActScrollLeft => {
                    //model.act_vertical_scroll(-1);
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActScrollRight => {
                    //model.act_vertical_scroll(1);
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActSelectAll => {
                    //model.act_select_all();
                    //model.print_info();
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActToggle => {
                    //model.act_toggle();
                    //model.print_info();
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActToggleAll => {
                    //model.act_toggle_all();
                    //model.print_info();
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActToggleDown => {
                    //model.act_toggle();
                    //model.act_move_line_cursor(-1);
                    //model.print_info();
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActToggleIn => {}

                //Event::EvActToggleOut => {}

                //Event::EvActToggleSort => {}

                //Event::EvActToggleUp => {
                    //model.act_toggle();
                    //model.act_move_line_cursor(1);
                    //model.print_info();
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActUnixLineDiscard => {
                    //model.act_line_discard();
                    //model.print_query();
                //}

                //Event::EvActUnixWordRubout => {
                    //model.act_backward_kill_word();
                    //model.print_query();
                //}

                //Event::EvActUp => {
                    //model.act_move_line_cursor(1);
                    //model.print_items();
                    //immediate_refresh = true;
                //}

                //Event::EvActYank => {}

                //_ => {
                    //printw(format!("{}\n", e as i32).as_str());
                //}
            //}
        //}

        //if immediate_refresh {
            //model.refresh();
        //} else {
            //model.refresh_throttle();
        //}
        //thread::sleep(Duration::from_millis(10));
    //};

    //model.close();
    //model.output();
    //exit_code
//}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}


use std::sync::mpsc::{sync_channel, channel, Sender, Receiver};
use std::io;
use model::ClosureType;


fn real_main() -> i32 {

    //------------------------------------------------------------------------------
    // parse options
    let mut opts = Options::new();
    opts.optopt("b", "bind", "comma seperated keybindings, such as 'ctrl-j:accept,ctrl-k:kill-line'", "KEY:ACTION");
    opts.optflag("h", "help", "print this help menu");
    opts.optflag("m", "multi", "Enable Multiple Selection");
    opts.optflag("", "no-multi", "Disable Multiple Selection");
    opts.optopt("p", "prompt", "prompt string", "'> '");
    opts.optopt("e", "expect", "comma seperated keys that can be used to complete fzf", "KEYS");
    opts.optopt("t", "tiebreak", "comma seperated criteria", "[score,index,begin,end,-score,...]");
    opts.optflag("", "ansi", "parse ANSI color codes for input strings");
    opts.optopt("c", "cmd", "command to invoke dynamically", "ag");
    opts.optflag("i", "interactive", "Use skim as an interactive interface");
    opts.optflag("", "regex", "use regex instead of fuzzy match");
    opts.optopt("q", "query", "specify the initial query", "\"\"");
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

    //------------------------------------------------------------------------------
    // query
    let mut query = query::Query::builder()
        .cmd("ls {}")
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
    thread::spawn(move || {
        model.run();
    });

    //------------------------------------------------------------------------------
    // input
    let (tx_input, rx_input) = channel();
    let tx_input_clone = tx_input.clone();
    let mut input = input::Input::new(tx_input_clone);
    thread::spawn(move || {
        input.run();
    });

    //------------------------------------------------------------------------------
    // start a timer for notifying refresh
    let tx_input_clone = tx_input.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(50));
            tx_input_clone.send((EvActRedraw, Box::new(true)));
        }
    });


    //------------------------------------------------------------------------------
    // now we can use
    // tx_reader: send message to reader
    // tx_model:  send message to model
    // rx_input:  receive keystroke events

    // light up the fire
    tx_reader.send((EvReaderRestart, Box::new((query.get_cmd(), query.get_query()))));

    let on_query_change = |query: &query::Query| {
        // restart the reader with new parameter
        tx_reader.send((EvReaderRestart, Box::new((query.get_cmd(), query.get_query()))));
        // send redraw event
        tx_input.send((EvActRedraw, Box::new(true)));
    };

    // listen user input
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

            EvActBackwardChar => {
                query.act_backward_char();
                let _ = tx_input.send((EvActRedraw, Box::new(true)));
            }

            EvActForwardChar => {
                query.act_forward_char();
                let _ = tx_input.send((EvActRedraw, Box::new(true)));
            }

            EvActRotateMode => {
                query.act_query_rotate_mode();
                let _ = tx_input.send((EvActRedraw, Box::new(true)));
            }

            EvActAccept => {
                // sync with model to quit

                let (tx, rx): (Sender<bool>, Receiver<bool>) = channel();
                let _ = tx_model.send((EvActAccept, Box::new(tx)));
                let _ = rx.recv();
                break;
            }

            EvActRedraw => {
                let _ = tx_model.send((EvModelRedraw, Box::new(query.get_print_func())));
            }

            EvActUp | EvActDown
                | EvActToggle | EvActToggleDown | EvActToggleUp => {
                let _ = tx_model.send((ev, arg));
            }

            _ => {}
        }
    }

    0
}

#![feature(io)]
extern crate libc;
extern crate ncurses;
extern crate getopts;
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

use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use std::sync::mpsc::channel;
use std::mem;
use std::ptr;
use util::eventbox::EventBox;

use ncurses::*;
use event::Event;
use input::Input;
use reader::Reader;
use matcher::Matcher;
use model::Model;
use libc::{sigemptyset, sigaddset, sigwait, pthread_sigmask};
use curses::{ColorTheme, Curses};
use getopts::Options;
use std::env;
use orderedvec::OrderedVec;
use item::MatchedItem;

fn main() {
    let exit_code = real_main();
    std::process::exit(exit_code);
}

fn real_main() -> i32 {

    // option parsing
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optopt("b", "bind", "comma seperated keybindings, such as 'ctrl-j:accept,ctrl-k:kill-line'", "KEY:ACTION");
    opts.optflag("h", "help", "print this help menu");
    opts.optflag("m", "multi", "Enable Multiple Selection");
    opts.optopt("p", "prompt", "prompt string", "'> '");
    opts.optopt("e", "expect", "comma seperated keys that can be used to complete fzf", "KEYS");

    let options = match opts.parse(&args[1..]) {
        Ok(m) => { m }
        Err(f) => { panic!(f.to_string()) }
    };

    // print help message
    if options.opt_present("h") {
        print_usage(&program, opts);
        return 0;
    }

    let theme = ColorTheme::new();
    let mut curses = Curses::new();
    curses.init(Some(&theme), false, false);

    // register terminal resize event, `pthread_sigmask` should be run before any thread.
    let mut sigset = unsafe {mem::uninitialized()};
    unsafe {
        sigemptyset(&mut sigset);
        sigaddset(&mut sigset, libc::SIGWINCH);
        pthread_sigmask(libc::SIG_SETMASK, &mut sigset, ptr::null_mut());
    }

    // controller variables
    let eb = Arc::new(EventBox::new());
    let item_buffer = Arc::new(RwLock::new(Vec::new()));

    let eb_clone = eb.clone();
    thread::spawn(move || {
        // listen to the resize event;
        loop {
            let mut sig = 0;
            let _errno = unsafe {sigwait(&mut sigset, &mut sig)};
            eb_clone.set(Event::EvResize, Box::new(true));
        }
    });

    // model
    let mut model = Model::new(eb.clone(), curses);
    // parse options for model
    if options.opt_present("m") {model.multi_selection = true;}
    if let Some(prompt) = options.opt_str("p") {model.prompt = prompt;}


    // matcher
    let items = model.items.clone();
    let mut matcher = Matcher::new(items, item_buffer.clone(), eb.clone());
    let eb_matcher = matcher.eb_req.clone();

    // reader
    let default_command = match env::var("FZF_DEFAULT_COMMAND") {
        Ok(val) => val,
        Err(_) => "find .".to_string(),
    };
    let mut reader = Reader::new(default_command, eb.clone(), item_buffer.clone());


    // input
    let mut input = Input::new(eb.clone());
    input.parse_keymap(options.opt_str("b"));
    input.parse_expect_keys(options.opt_str("e"));

    // start running
    thread::spawn(move || {
        reader.run();
    });

    thread::spawn(move || {
        matcher.run();
    });

    thread::spawn(move || {
        input.run();
    });

    let mut exit_code = 0;

    'outer: loop {
        for (e, val) in eb.wait() {
            match e {
                Event::EvReaderNewItem => {
                    eb_matcher.set(Event::EvMatcherNewItem, Box::new(true));
                    let reading: bool = *val.downcast().unwrap();
                    model.reading = reading;

                    let new_items = item_buffer.read().unwrap();
                    model.update_num_total(new_items.len());
                    model.print_info();
                }

                Event::EvMatcherUpdateProcess => {
                    let percentage: u64 = *val.downcast().unwrap();
                    model.update_percentage(percentage);
                    model.print_info();
                }

                Event::EvMatcherEnd => {
                    // do nothing
                    let result: OrderedVec<MatchedItem> = *val.downcast().unwrap();
                    model.update_matched_items(result);
                    model.display();
                }

                Event::EvQueryChange => {
                    eb_matcher.set(Event::EvMatcherResetQuery, val);
                }

                Event::EvInputInvalid => {
                    // ignore
                }

                Event::EvInputKey => {
                    // ignore for now
                }

                Event::EvResize => {model.resize(); model.display();}

                Event::EvActAddChar => {
                    let ch: char = *val.downcast().unwrap();
                    model.act_add_char(ch);
                    model.print_query();
                }

                // Actions
                Event::EvActAbort => {exit_code = 130; break 'outer; }
                Event::EvActAccept => {
                    // break out of the loop and output the selected item.
                    //if model.get_num_selected() <= 0 { model.act_toggle(Some(true)); }
                    let args: Option<String> = *val.downcast().unwrap();
                    let num_selected = model.act_accept(args);
                    exit_code = if num_selected > 0 {0} else {1};
                    break 'outer;
                }
                Event::EvActBackwardChar       => {model.act_backward_char();       model.print_query();}
                Event::EvActBackwardDeleteChar => {model.act_backward_delete_char();model.print_query();}
                Event::EvActBackwardKillWord   => {model.act_backward_kill_word();  model.print_query();}
                Event::EvActBackwardWord       => {model.act_backward_word();       model.print_query();}
                Event::EvActBeginningOfLine    => {model.act_beginning_of_line();   model.print_query();}
                Event::EvActCancel             => {}
                Event::EvActClearScreen        => {model.refresh();}
                Event::EvActDeleteChar         => {model.act_delete_char();         model.print_query();}
                Event::EvActDeleteCharEOF      => {model.act_delete_char();         model.print_query();}
                Event::EvActDeselectAll        => {model.act_deselect_all();        model.print_info(); model.print_items();}
                Event::EvActDown               => {model.act_move_line_cursor(-1);  model.print_items();}
                Event::EvActEndOfLine          => {model.act_end_of_line();         model.print_query();}
                Event::EvActForwardChar        => {model.act_forward_char();        model.print_query();}
                Event::EvActForwardWord        => {model.act_forward_word();        model.print_query();}
                Event::EvActIgnore             => {}
                Event::EvActKillLine           => {model.act_kill_line();           model.print_query();}
                Event::EvActKillWord           => {model.act_kill_word();           model.print_query();}
                Event::EvActNextHistory        => {}
                Event::EvActPageDown           => {model.act_move_page(-1);         model.print_items();}
                Event::EvActPageUp             => {model.act_move_page(1);          model.print_items();}
                Event::EvActPreviousHistory    => {}
                Event::EvActScrollLeft         => {model.act_vertical_scroll(-1);   model.print_items();}
                Event::EvActScrollRight        => {model.act_vertical_scroll(1);    model.print_items();}
                Event::EvActSelectAll          => {model.act_select_all();          model.print_info(); model.print_items();}
                Event::EvActToggle             => {model.act_toggle();              model.print_info(); model.print_items();}
                Event::EvActToggleAll          => {model.act_toggle_all();          model.print_info(); model.print_items();}
                Event::EvActToggleDown         => {
                    model.act_toggle();
                    model.act_move_line_cursor(-1);
                    model.print_info();
                    model.print_items();
                }
                Event::EvActToggleIn           => {}
                Event::EvActToggleOut          => {}
                Event::EvActToggleSort         => {}
                Event::EvActToggleUp           => {
                    model.act_toggle();
                    model.act_move_line_cursor(1);
                    model.print_info();
                    model.print_items();
                }
                Event::EvActUnixLineDiscard    => {model.act_line_discard();       model.print_query();}
                Event::EvActUnixWordRubout     => {model.act_backward_kill_word(); model.print_query();}
                Event::EvActUp                 => {model.act_move_line_cursor(1);  model.print_items();}
                Event::EvActYank               => {}

                _ => {
                    printw(format!("{}\n", e as i32).as_str());
                }
            }

            model.refresh();
            thread::sleep(Duration::from_millis(10));
        }
    };

    model.close();
    model.output();
    return exit_code;
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}

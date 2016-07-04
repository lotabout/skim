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

use std::sync::Arc;
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
    let mut curse = Curses::new();
    curse.init(Some(&theme), false, false);

    let eb = Arc::new(EventBox::new());

    // register terminal resize event, `pthread_sigmask` should be run before any thread.
    let mut sigset = unsafe {mem::uninitialized()};
    unsafe {
        sigemptyset(&mut sigset);
        sigaddset(&mut sigset, libc::SIGWINCH);
        pthread_sigmask(libc::SIG_SETMASK, &mut sigset, ptr::null_mut());
    }

    let eb_clone_resize = eb.clone();
    thread::spawn(move || {
        // listen to the resize event;
        loop {
            let mut sig = 0;
            let _errno = unsafe {sigwait(&mut sigset, &mut sig)};
            eb_clone_resize.set(Event::EvResize, Box::new(true));
        }
    });

    let (tx_matched, rx_matched) = channel();
    let eb_model = eb.clone();
    let mut model = Model::new(eb_model, curse);
    // parse options for model
    if options.opt_present("m") {model.multi_selection = true;}
    if let Some(prompt) = options.opt_str("p") {model.prompt = prompt;}


    let eb_matcher = Arc::new(EventBox::new());
    let eb_matcher_clone = eb_matcher.clone();
    let eb_clone_matcher = eb.clone();
    let items = model.items.clone();
    let mut matcher = Matcher::new(items, tx_matched, eb_matcher_clone, eb_clone_matcher);

    let eb_clone_reader = eb.clone();
    let items = model.items.clone();
    let default_command = match env::var("FZF_DEFAULT_COMMAND") {
        Ok(val) => val,
        Err(_) => "find .".to_string(),
    };
    let mut reader = Reader::new(default_command, eb_clone_reader, items);


    let eb_clone_input = eb.clone();
    let mut input = Input::new(eb_clone_input);
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
        let mut need_refresh = true;
        for (e, val) in eb.wait() {
            match e {
                Event::EvReaderNewItem => {
                    eb_matcher.set(Event::EvMatcherNewItem, Box::new(true));
                    let reading: bool = *val.downcast().unwrap();
                    model.reading = reading;
                }

                Event::EvMatcherUpdateProcess => {
                    let (matched, total, processed) : (u64, u64, u64) = *val.downcast().unwrap();
                    model.update_process_info(matched, total, processed);

                    while let Ok(matched_item) = rx_matched.try_recv() {
                        model.push_item(matched_item);
                    }
                }

                Event::EvMatcherStart => {
                    while let Ok(_) = rx_matched.try_recv() {}
                    model.clear_items();
                    eb_matcher.set(Event::EvMatcherStartReceived, Box::new(true));
                    need_refresh = false;
                }

                Event::EvMatcherEnd => {
                    // do nothing
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

                Event::EvResize => {model.resize();}

                Event::EvActAddChar => {
                    let ch: char = *val.downcast().unwrap();
                    model.act_add_char(ch);
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
                Event::EvActBackwardChar       => {model.act_backward_char();}
                Event::EvActBackwardDeleteChar => {model.act_backward_delete_char();}
                Event::EvActBackwardKillWord   => {model.act_backward_kill_word();}
                Event::EvActBackwardWord       => {model.act_backward_word();}
                Event::EvActBeginningOfLine    => {model.act_beginning_of_line();}
                Event::EvActCancel             => {}
                Event::EvActClearScreen        => {model.refresh();}
                Event::EvActDeleteChar         => {model.act_delete_char();}
                Event::EvActDeleteCharEOF      => {model.act_delete_char();}
                Event::EvActDeselectAll        => {model.act_deselect_all();}
                Event::EvActDown               => {model.act_move_line_cursor(-1);}
                Event::EvActEndOfLine          => {model.act_end_of_line();}
                Event::EvActForwardChar        => {model.act_forward_char();}
                Event::EvActForwardWord        => {model.act_forward_word();}
                Event::EvActIgnore             => {}
                Event::EvActKillLine           => {model.act_kill_line();}
                Event::EvActKillWord           => {model.act_kill_word();}
                Event::EvActNextHistory        => {}
                Event::EvActPageDown           => {model.act_move_page(-1);}
                Event::EvActPageUp             => {model.act_move_page(1);}
                Event::EvActPreviousHistory    => {}
                Event::EvActScrollLeft         => {model.act_vertical_scroll(-1);}
                Event::EvActScrollRight        => {model.act_vertical_scroll(1);}
                Event::EvActSelectAll          => {model.act_select_all();}
                Event::EvActToggle             => {model.act_toggle();}
                Event::EvActToggleAll          => {model.act_toggle_all();}
                Event::EvActToggleDown         => {
                    model.act_toggle();
                    model.act_move_line_cursor(-1);
                }
                Event::EvActToggleIn           => {}
                Event::EvActToggleOut          => {}
                Event::EvActToggleSort         => {}
                Event::EvActToggleUp           => {
                    model.act_toggle();
                    model.act_move_line_cursor(1);
                }
                Event::EvActUnixLineDiscard    => {model.act_line_discard();}
                Event::EvActUnixWordRubout     => {model.act_backward_kill_word();}
                Event::EvActUp                 => {model.act_move_line_cursor(1);}
                Event::EvActYank               => {}

                _ => {
                    printw(format!("{}\n", e as i32).as_str());
                }
            }
        }

        thread::sleep(Duration::from_millis(10));
        model.display();
        if need_refresh {
            model.refresh();
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

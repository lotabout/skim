#![feature(io)]
extern crate libc;
extern crate ncurses;
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

fn main() {

    let theme = curses::ColorTheme::new();
    let mut curse = Curses::new();
    curse.init(Some(&theme), false, false);

    let eb = Arc::new(EventBox::new());
    let (tx_matched, rx_matched) = channel();
    let eb_model = eb.clone();
    let mut model = Model::new(eb_model, curse);

    let eb_matcher = Arc::new(EventBox::new());
    let eb_matcher_clone = eb_matcher.clone();
    let eb_clone_matcher = eb.clone();
    let items = model.items.clone();
    let mut matcher = Matcher::new(items, tx_matched, eb_matcher_clone, eb_clone_matcher);

    let eb_clone_reader = eb.clone();
    let items = model.items.clone();
    let mut reader = Reader::new(Some(&"find ."), eb_clone_reader, items);


    let eb_clone_input = eb.clone();
    let mut input = Input::new(eb_clone_input);

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

    'outer: loop {
        let mut need_refresh = true;
        for (e, val) in eb.wait() {
            match e {
                Event::EvReaderNewItem | Event::EvReaderFinished => {
                    eb_matcher.set(Event::EvMatcherNewItem, Box::new(true));
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

                Event::EvResize => { model.resize(); }

                // Actions
                Event::EvActAddChar => {
                    let ch: char = *val.downcast().unwrap();
                    model.act_add_char(ch);
                }
                Event::EvActToggleDown => {
                    model.toggle_select(None);
                    model.move_line_cursor(1);
                }
                Event::EvActUp => { model.move_line_cursor(-1); }
                Event::EvActDown => { model.move_line_cursor(1); }
                Event::EvActBackwardChar => { model.act_backward_char(); }
                Event::EvActBackwardDeleteChar => { model.act_backward_delete_char(); }
                Event::EvActSelect => {
                    // break out of the loop and output the selected item.
                    if model.get_num_selected() <= 0 { model.toggle_select(Some(true)); }
                    break 'outer;
                }
                Event::EvActQuit => { break 'outer; }

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

    endwin();
    model.output();
}

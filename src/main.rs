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

use std::sync::Arc;
use std::thread;
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

    let mut model = Model::new(curse);
    let eb = Arc::new(EventBox::new());
    let (tx_source, rx_source) = channel();
    let (tx_matched, rx_matched) = channel();

    let eb_clone_reader = eb.clone();
    let mut reader = Reader::new(Some(&"find ."), eb_clone_reader, tx_source);

    let eb_matcher = Arc::new(EventBox::new());
    let eb_matcher_clone = eb_matcher.clone();
    let eb_clone_matcher = eb.clone();
    let items = model.items.clone();
    let mut matcher = Matcher::new(items, tx_matched, eb_matcher_clone, eb_clone_matcher);

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
        for (e, val) in eb.wait() {
            match e {
                Event::EvReaderNewItem | Event::EvReaderFinished => {
                    eb_matcher.set(Event::EvMatcherNewItem, Box::new(true));
                    let mut items = model.items.write().unwrap();
                    while let Ok(string) = rx_source.try_recv() {
                        items.push(string);
                    }
                }

                Event::EvMatcherUpdateProcess => {
                    let (matched, total) : (u64, u64) = *val.downcast().unwrap();
                    model.update_process_info(matched, total);

                    while let Ok(matched_item) = rx_matched.try_recv() {
                        model.push_item(matched_item);
                    }
                }

                Event::EvQueryChange => {
                    let (query, pos) : (String, usize) = *val.downcast().unwrap();
                    let modified = query != model.query;
                    model.update_query(query.clone(), pos as i32);

                    if modified {
                        model.clear_items();
                        eb_matcher.set(Event::EvMatcherResetQuery, Box::new(model.query.clone()));
                    }
                }

                Event::EvInputSelect => {
                    // break out of the loop and output the selected item.
                    model.toggle_select(Some(true));
                    break 'outer;
                }

                Event::EvInputToggle => {
                    model.toggle_select(None);
                    model.move_line_cursor(1);
                }
                Event::EvInputUp=> {
                    model.move_line_cursor(-1);
                }
                Event::EvInputDown=> {
                    model.move_line_cursor(1);
                }
                Event::EvResize => {
                    model.resize();
                }

                _ => {
                    printw(format!("{}\n", e as i32).as_str());
                }
            }
        }
        model.display();
        refresh();
    };

    endwin();
    model.output();
}

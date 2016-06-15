/// Input will listens to user input, modify the query string, send special
/// keystrokes(such as Enter, Ctrl-p, Ctrl-n, etc) to the controller.

use std::sync::Arc;
use std::char;

use util::eventbox::EventBox;
use event::Event;

use ncurses::*;

pub struct Input {
    query: Vec<char>,
    index: usize, // index in chars
    pos: usize, // position in bytes
    eb: Arc<EventBox<Event>>,
}

impl Input {
    pub fn new(eb: Arc<EventBox<Event>>) -> Self {
        Input {
            query: Vec::new(),
            index: 0,
            pos: 0,
            eb: eb,
        }
    }

    fn get_query(&self) -> String {
        self.query.iter().cloned().collect::<String>()
    }

    fn add_char (&mut self, ch: char) {
        self.query.insert(self.index, ch);
        self.index += 1;
        self.pos += if ch.len_utf8() > 1 {2} else {1};
    }

    fn delete_char(&mut self) {
        if self.index == 0 {
            return;
        }

        let ch = self.query.remove(self.index-1);
        self.index -= 1;
        self.pos -= if ch.len_utf8() > 1 {2} else {1};
    }

    pub fn run(&mut self) {
        loop {
            self.handle_char();
        }
    }

    // fetch input from curses and turn it into query.
    fn handle_char(&mut self) {
        let ch = wget_wch(stdscr);

        match ch {
            Some(WchResult::KeyCode(_)) => {
                // will later handle readline-like shortcuts
            }

            Some(WchResult::Char(c)) => {
                /* Enable attributes and output message. */
                let ch = char::from_u32(c as u32).expect("Invalid char");
                match ch {
                    '\x7F' => { // backspace
                        self.delete_char();
                        self.eb.set(Event::EvQueryChange, Box::new((self.get_query(), self.pos)));
                    }

                    '\x0A' => { // enter
                        self.eb.set(Event::EvInputSelect, Box::new(true));
                    }

                    '\x09' => { // tab
                        self.eb.set(Event::EvInputToggle, Box::new(true));
                    }

                    '\x10' => { // ctrl-p
                        self.eb.set(Event::EvInputUp, Box::new(true));
                    }

                    '\x0E' => { // ctrl-n
                        self.eb.set(Event::EvInputDown, Box::new(true));
                    }

                    ch => { // other characters
                        self.add_char(ch);
                        self.eb.set(Event::EvQueryChange, Box::new((self.get_query(), self.pos)));
                    }
                }
            }

            None => { }
        }
    }
}


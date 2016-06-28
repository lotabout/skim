/// Input will listens to user input, modify the query string, send special
/// keystrokes(such as Enter, Ctrl-p, Ctrl-n, etc) to the controller.

use std::sync::Arc;
use std::char;
use std::io::prelude::*;
use std::fs::File;
use std::collections::HashMap;

use util::eventbox::EventBox;
use event::Event;

use ncurses::*;

pub struct Input {
    query: Vec<char>,
    index: usize, // index in chars
    pos: usize, // position in bytes
    eb: Arc<EventBox<Event>>,
    actions: HashMap<&'static str, fn(&mut Input)>,
}

impl Input {
    pub fn new(eb: Arc<EventBox<Event>>) -> Self {
        Input {
            query: Vec::new(),
            index: 0,
            pos: 0,
            eb: eb,
            actions: get_action_table(),
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
        let f = File::open("/dev/tty").unwrap();
        for c in f.chars() {
            self.handle_char(c.unwrap());
        }
    }

    // fetch input from curses and turn it into query.
    fn handle_char(&mut self, ch: char) {
        match ch {
            '\x7F' => { // backspace
                self.actions.get("delete_char").unwrap()(self);
            }

            '\x0D' => { // enter
                self.eb.set(Event::EvInputSelect, Box::new(true));
            }

            '\x09' => { // tab
                self.eb.set(Event::EvInputToggle, Box::new(true));
            }

            '\x10' => { // ctrl-p
                self.eb.set(Event::EvInputUp, Box::new(true));
            }

            //'\x0E' => { // ctrl-n
                //self.eb.set(Event::EvInputDown, Box::new(true));
            //}

            ch => { // other characters
                for c in ch.escape_unicode() {
                    print!("{}", c);
                }
                println!("");
                mv(0,0);
                refresh();
                //self.add_char(ch);
                //self.eb.set(Event::EvQueryChange, Box::new((self.get_query(), self.pos)));
            }
        }
    }
}

pub enum Key {
    CtrlA,
    CtrlB,
    CtrlC,
    CtrlD,
    CtrlE,
    CtrlF,
    CtrlG,
    CtrlH,
    Tab,
    CtrlJ,
    CtrlK,
    CtrlL,
    CtrlM,
    CtrlN,
    CtrlO,
    CtrlP,
    CtrlQ,
    CtrlR,
    CtrlS,
    CtrlT,
    CtrlU,
    CtrlV,
    CtrlW,
    CtrlX,
    CtrlY,
    CtrlZ,
    ESC,

    Invalid,
    Mouse,
    DoubleClick,

    BTab,
    BSpace,

    Del,
    PgUp,
    PgDn,

    Up,
    Down,
    Left,
    Right,
    Home,
    End,

    SLeft,
    SRight,

    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,

    AltEnter,
    AltSpace,
    AltSlash,
    AltBS,
    AltA,
    AltB,
    AltC,
    AltD,
    AltE,
    AltF,
    AltZ,
}

fn get_key_table() -> HashMap<char, Key> {
    let mut table = HashMap::new();

    table.insert('\x01', Key::CtrlA);
    table.insert('\x02', Key::CtrlB);
    table.insert('\x03', Key::CtrlC);
    table.insert('\x04', Key::CtrlD);
    table.insert('\x05', Key::CtrlE);
    table.insert('\x06', Key::CtrlF);
    table.insert('\x07', Key::CtrlG);
    table.insert('\x08', Key::CtrlH);
    table.insert('\x09', Key::Tab);
    table.insert('\x0a', Key::CtrlJ);
    table.insert('\x0b', Key::CtrlK);
    table.insert('\x0c', Key::CtrlL);
    table.insert('\x0d', Key::CtrlM);
    table.insert('\x0e', Key::CtrlN);
    table.insert('\x0f', Key::CtrlO);
    table.insert('\x10', Key::CtrlP);
    table.insert('\x11', Key::CtrlQ);
    table.insert('\x12', Key::CtrlR);
    table.insert('\x13', Key::CtrlS);
    table.insert('\x14', Key::CtrlT);
    table.insert('\x15', Key::CtrlU);
    table.insert('\x16', Key::CtrlV);
    table.insert('\x17', Key::CtrlW);
    table.insert('\x18', Key::CtrlX);
    table.insert('\x19', Key::CtrlY);
    table.insert('\x1a', Key::CtrlZ);
    table.insert('\x1b', Key::ESC);

    table.insert('\x00', Key::Invalid);
    table.insert('\x00', Key::Mouse);
    table.insert('\x00', Key::DoubleClick);

    table.insert('\x10', Key::BTab);
    table.insert('\x6b', Key::BSpace);

    table.insert('\x00', Key::Del);
    table.insert('\x00', Key::PgUp);
    table.insert('\x00', Key::PgDn);

    table.insert('\x00', Key::Up);
    table.insert('\x00', Key::Down);
    table.insert('\x00', Key::Left);
    table.insert('\x00', Key::Right);
    table.insert('\x00', Key::Home);
    table.insert('\x00', Key::End);

    table.insert('\x00', Key::SLeft);
    table.insert('\x00', Key::SRight);

    table.insert('\x00', Key::F1);
    table.insert('\x00', Key::F2);
    table.insert('\x00', Key::F3);
    table.insert('\x00', Key::F4);
    table.insert('\x00', Key::F5);
    table.insert('\x00', Key::F6);
    table.insert('\x00', Key::F7);
    table.insert('\x00', Key::F8);
    table.insert('\x00', Key::F9);
    table.insert('\x00', Key::F10);

    table.insert('\x00', Key::AltEnter);
    table.insert('\x00', Key::AltSpace);
    table.insert('\x00', Key::AltSlash);
    table.insert('\x00', Key::AltBS);
    table.insert('\x00', Key::AltA);
    table.insert('\x00', Key::AltB);
    table.insert('\x00', Key::AltC);
    table.insert('\x00', Key::AltD);
    table.insert('\x00', Key::AltE);
    table.insert('\x00', Key::AltF);
    table.insert('\x00', Key::AltZ);

    table
}

// all actions
fn act_delete_char(input: &mut Input) {
    input.delete_char();
    input.eb.set(Event::EvQueryChange, Box::new((input.get_query(), input.pos)));
}

fn act_select(input: &mut Input) {
    input.eb.set(Event::EvInputSelect, Box::new(true));
}

fn act_toggle(input: &mut Input) {
    input.eb.set(Event::EvInputToggle, Box::new(true));
}

fn act_up(input: &mut Input) {
    input.eb.set(Event::EvInputUp, Box::new(true));
}
fn act_down(input: &mut Input) {
    input.eb.set(Event::EvInputDown, Box::new(true));
}

fn get_action_table() -> HashMap<&'static str, fn(&mut Input)> {
    let mut map: HashMap<&'static str, fn(&mut Input)> = HashMap::new();
    map.insert("delete_char", act_delete_char);
    map.insert("select", act_select);
    map.insert("toggle", act_toggle);
    map.insert("up", act_up);
    map.insert("down", act_down);
    map
}

/// Input will listens to user input, modify the query string, send special
/// keystrokes(such as Enter, Ctrl-p, Ctrl-n, etc) to the controller.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::io::prelude::*;
use std::fs::File;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::Duration;
use utf8parse;

use event::{Event, EventSender, parse_action};

pub struct Input {
    tx_input: EventSender,
    keyboard: KeyBoard,
    keymap: HashMap<Key, (Event, Option<String>)>,
}

impl Input {
    pub fn new(tx_input: EventSender) -> Self {
        let f = File::open("/dev/tty").unwrap();
        let keyboard = KeyBoard::new(f);
        Input {
            tx_input: tx_input,
            keyboard: keyboard,
            keymap: get_default_key_map(),
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.keyboard.get_key() {
                Some(Key::Pos(row, col)) => {
                    let _ = self.tx_input.send((Event::EvReportCursorPos, Box::new((row, col))));
                }
                Some(key) => {
                    // search event from keymap
                    match self.keymap.get(&key) {
                        Some(&(ev, Some(ref args))) => {
                            let _ = self.tx_input.send((ev, Box::new(Some(args.clone()))));
                        }
                        Some(&(ev, None)) => {
                            let _ = self.tx_input.send((ev, Box::new(None as Option<String>)));
                        }
                        None => {
                            if let Key::Char(ch) = key {
                                let _ = self.tx_input.send((Event::EvActAddChar, Box::new(ch)));
                            } else {
                                let _ = self.tx_input.send((Event::EvInputKey, Box::new(key)));
                            }
                        }
                    }
                }
                None => {
                    let _ = self.tx_input.send((Event::EvInputInvalid, Box::new(true)));
                }
            }
        }
    }

    pub fn bind(&mut self, key: &str, action: &str, args: Option<String>) {
        let key = parse_key(key);
        let action = parse_action(action);
        if key == None || action == None {return;}

        let key = key.unwrap();
        let act = action.unwrap();

        // remove the key for existing keymap;
        let _ = self.keymap.remove(&key);
        self.keymap.entry(key).or_insert((act, args));
    }

    pub fn parse_keymaps(&mut self, maps: &[&str]) {
        for &map in maps {
            self.parse_keymap(map);
        }
    }

    // key_action is comma separated: 'ctrl-j:accept,ctrl-k:kill-line'
    pub fn parse_keymap(&mut self, key_action: &str) {
        for pair in key_action.split(',') {
            let vec: Vec<&str> = pair.split(':').collect();
            if vec.len() < 2 {
                continue;
            }
            self.bind(vec[0], vec[1], vec.get(2).map(|&string| string.to_string()));
        }
    }

    pub fn parse_expect_keys(&mut self, keys: Option<&str>) {
        if let Some(keys) = keys {
            self.bind("enter", "accept", Some("".to_string()));
            for key in keys.split(',') {
                self.bind(key, "accept", Some(key.to_string()));
            }
        }
    }
}

// check https://github.com/rust-lang/rust/issues/27802#issuecomment-270555935
struct SimpleUtf8Receiver {
    tx: Sender<char>,
}

impl SimpleUtf8Receiver {
    pub fn new(tx: Sender<char>) -> Self {
        SimpleUtf8Receiver {
            tx: tx,
        }
    }
}

impl utf8parse::Receiver for SimpleUtf8Receiver {
    fn codepoint(&mut self, ch: char) {
        let _ = self.tx.send(ch);
    }

    fn invalid_sequence(&mut self) {
        // ignore it
    }
}


struct KeyBoard {
    rx: Receiver<char>,
    buf: VecDeque<char>,
}

impl KeyBoard {
    pub fn new(f: File) -> Self {
        let (tx, rx) = channel();
        thread::spawn(move || {
            let mut utf8_receiver = SimpleUtf8Receiver::new(tx);
            let mut utf8_parser = utf8parse::Parser::new();
            for byte in f.bytes() {
                utf8_parser.advance(&mut utf8_receiver, byte.unwrap());
            }
        });

        KeyBoard {
            rx: rx,
            buf: VecDeque::new(),
        }
    }

    fn getch(&self, is_block: bool) -> Option<char> {
        if is_block {
            self.rx.recv().ok()
        } else {
            self.rx.try_recv().ok()
        }
    }

    fn get_chars(&mut self) {
        let ch = self.getch(true).unwrap();
        self.buf.push_back(ch);

        // sleep for a short time to make sure the chars(if any) are ready to read.
        thread::sleep(Duration::from_millis(1));
        while let Some(ch) = self.getch(false) {
            self.buf.push_back(ch);
        }
    }

    pub fn get_key(&mut self) -> Option<Key> {
        if self.buf.is_empty() {
            self.get_chars();
        }

        let ch = self.buf.pop_front();

        match ch {
            Some('\u{00}') => Some(Key::Char(' ')),
            Some('\u{01}') => Some(Key::CtrlA),
            Some('\u{02}') => Some(Key::CtrlB),
            Some('\u{03}') => Some(Key::CtrlC),
            Some('\u{04}') => Some(Key::CtrlD),
            Some('\u{05}') => Some(Key::CtrlE),
            Some('\u{06}') => Some(Key::CtrlF),
            Some('\u{07}') => Some(Key::CtrlG),
            Some('\u{08}') => Some(Key::CtrlH),
            Some('\u{09}') => Some(Key::Tab),
            Some('\u{0A}') => Some(Key::CtrlJ),
            Some('\u{0B}') => Some(Key::CtrlK),
            Some('\u{0C}') => Some(Key::CtrlL),
            Some('\u{0D}') => Some(Key::Enter),
            Some('\u{0E}') => Some(Key::CtrlN),
            Some('\u{0F}') => Some(Key::CtrlO),
            Some('\u{10}') => Some(Key::CtrlP),
            Some('\u{11}') => Some(Key::CtrlQ),
            Some('\u{12}') => Some(Key::CtrlR),
            Some('\u{13}') => Some(Key::CtrlS),
            Some('\u{14}') => Some(Key::CtrlT),
            Some('\u{15}') => Some(Key::CtrlU),
            Some('\u{16}') => Some(Key::CtrlV),
            Some('\u{17}') => Some(Key::CtrlW),
            Some('\u{18}') => Some(Key::CtrlX),
            Some('\u{19}') => Some(Key::CtrlY),
            Some('\u{1A}') => Some(Key::CtrlZ),
            Some('\u{1B}') => self.get_escaped_key(),

            Some('\u{7F}') => Some(Key::BSpace),

            Some(c) => {Some(Key::Char(c))}
            None => None
        }
    }

    fn get_escaped_key(&mut self) -> Option<Key>{
        let ch = self.buf.pop_front();
        match ch {
            Some('\u{0D}') => Some(Key::AltEnter),
            Some(' ')      => Some(Key::AltSpace),
            Some('/')      => Some(Key::AltSlash),
            Some('\u{7F}') => Some(Key::AltBS),
            Some('a')      => Some(Key::AltA),
            Some('b')      => Some(Key::AltB),
            Some('c')      => Some(Key::AltC),
            Some('d')      => Some(Key::AltD),
            Some('e')      => Some(Key::AltE),
            Some('f')      => Some(Key::AltF),
            Some('g')      => Some(Key::AltG),
            Some('h')      => Some(Key::AltH),
            Some('i')      => Some(Key::AltI),
            Some('j')      => Some(Key::AltJ),
            Some('k')      => Some(Key::AltK),
            Some('l')      => Some(Key::AltL),
            Some('m')      => Some(Key::AltM),
            Some('n')      => Some(Key::AltN),
            Some('o')      => Some(Key::AltO),
            Some('p')      => Some(Key::AltP),
            Some('q')      => Some(Key::AltQ),
            Some('r')      => Some(Key::AltR),
            Some('s')      => Some(Key::AltS),
            Some('t')      => Some(Key::AltT),
            Some('u')      => Some(Key::AltU),
            Some('v')      => Some(Key::AltV),
            Some('w')      => Some(Key::AltW),
            Some('x')      => Some(Key::AltX),
            Some('y')      => Some(Key::AltY), // -> \u{79}
            Some('z')      => Some(Key::AltZ),

            Some(c @ '\u{5B}') | Some( c @ '\u{4F}') => {
                // try to match ^[{row};{col}R pattern first
                if c == '\u{5B}' && self.buf.contains(&';') && self.buf.contains(&'R') {
                    let mut row = String::new();
                    let mut col = String::new();
                    while self.buf.front() != Some(&';') {
                        row.push(self.buf.pop_front().unwrap());
                    }
                    self.buf.pop_front();
                    while self.buf.front() != Some(&'R') {
                        col.push(self.buf.pop_front().unwrap());
                    }
                    self.buf.pop_front();

                    return Some(Key::Pos(row.parse::<u16>().unwrap() - 1, col.parse::<u16>().unwrap() - 1));
                }

                // other special sequence
                let ch = self.buf.pop_front();
                match ch {
                    Some('\u{41}') => Some(Key::Up),
                    Some('\u{42}') => Some(Key::Down),
                    Some('\u{44}') => Some(Key::Left),
                    Some('\u{43}') => Some(Key::Right),
                    Some('\u{5A}') => Some(Key::BTab),
                    Some('\u{48}') => Some(Key::Home),
                    Some('\u{46}') => Some(Key::End),
                    Some('\u{4D}') => None, // mouse sequence
                    Some('\u{50}') => Some(Key::F1),
                    Some('\u{51}') => Some(Key::F2),
                    Some('\u{52}') => Some(Key::F3),
                    Some('\u{53}') => Some(Key::F4),
                    Some('\u{31}') => {
                        match self.buf.pop_front() {
                            Some('\u{7e}') => Some(Key::Home),
                            Some('\u{35}') => {
                                if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::F5)} else {None}
                            }
                            Some('\u{37}') => {
                                if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::F6)} else {None}
                            }
                            Some('\u{38}') => {
                                if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::F7)} else {None}
                            }
                            Some('\u{39}') => {
                                if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::F8)} else {None}
                            }
                            Some('\u{3B}') => {
                                match self.buf.pop_front() {
                                    Some('\u{32}') => {
                                        match self.buf.pop_front() {
                                            Some('\u{44}') => Some(Key::Home),
                                            Some('\u{43}') => Some(Key::End),
                                            Some(_) | None => None
                                        }
                                    }
                                    Some('\u{35}') => {
                                        match self.buf.pop_front() {
                                            Some('\u{44}') => Some(Key::SLeft),
                                            Some('\u{43}') => Some(Key::SRight),
                                            Some(_) | None => None
                                        }
                                    }
                                    Some(_) | None => None
                                }
                            }
                            Some(_) | None => None
                        }
                    }
                    Some('\u{32}') => {
                        match self.buf.pop_front() {
                            Some('\u{30}') => {
                                if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::F9)} else {None}
                            }
                            Some('\u{31}') => {
                                if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::F10)} else {None}
                            }
                            Some('\u{33}') => {
                                if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::F11)} else {None}
                            }
                            Some('\u{34}') => {
                                if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::F12)} else {None}
                            }
                            Some(_) | None => None
                        }
                    }
                    Some('\u{33}') => {
                        if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::Del)} else {None}
                    }
                    Some('\u{34}') => {
                        if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::End)} else {None}
                    }
                    Some('\u{35}') => {
                        if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::PgUp)} else {None}
                    }
                    Some('\u{36}') => {
                        if let Some('\u{7e}') = self.buf.pop_front() {Some(Key::PgDn)} else {None}
                    }
                    Some(_) | None => None
                }
            }
            Some(c) => {
                // not matched escaped sequence.
                self.buf.push_front(c);
                Some(Key::ESC)
            }
            None => Some(Key::ESC),
        }
    }
}


#[derive(Eq, PartialEq, Hash, Debug)]
pub enum Key {
    CtrlA, CtrlB, CtrlC, CtrlD, CtrlE, CtrlF, CtrlG, CtrlH, Tab,   CtrlJ, CtrlK, CtrlL, Enter,
    CtrlN, CtrlO, CtrlP, CtrlQ, CtrlR, CtrlS, CtrlT, CtrlU, CtrlV, CtrlW, CtrlX, CtrlY, CtrlZ,
    ESC,

    Mouse,
    DoubleClick,

    BTab,
    BSpace,

    Del, PgUp, PgDn,

    Up, Down, Left, Right, Home, End,

    SLeft, SRight,

    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,

    AltEnter,
    AltSpace,
    AltSlash,
    AltBS,

    AltA, AltB, AltC, AltD, AltE, AltF, AltG, AltH, AltI, AltJ, AltK, AltL, AltM,
    AltN, AltO, AltP, AltQ, AltR, AltS, AltT, AltU, AltV, AltW, AltX, AltY, AltZ,
    Char(char),
    Pos(u16, u16),
}

pub fn parse_key(key: &str) -> Option<Key> {
    match key.to_lowercase().as_ref() {
        "ctrl-a" => Some(Key::CtrlA),
        "ctrl-b" => Some(Key::CtrlB),
        "ctrl-c" => Some(Key::CtrlC),
        "ctrl-d" => Some(Key::CtrlD),
        "ctrl-e" => Some(Key::CtrlE),
        "ctrl-f" => Some(Key::CtrlF),
        "ctrl-g" => Some(Key::CtrlG),
        "ctrl-h" => Some(Key::CtrlH),
        "tab" | "ctrl-i" => Some(Key::Tab),
        "ctrl-j" => Some(Key::CtrlJ),
        "ctrl-k" => Some(Key::CtrlK),
        "ctrl-l" => Some(Key::CtrlL),
        "enter" | "return" | "ctrl-m" => Some(Key::Enter),
        "ctrl-n" => Some(Key::CtrlN),
        "ctrl-o" => Some(Key::CtrlO),
        "ctrl-p" => Some(Key::CtrlP),
        "ctrl-q" => Some(Key::CtrlQ),
        "ctrl-r" => Some(Key::CtrlR),
        "ctrl-s" => Some(Key::CtrlS),
        "ctrl-t" => Some(Key::CtrlT),
        "ctrl-u" => Some(Key::CtrlU),
        "ctrl-v" => Some(Key::CtrlV),
        "ctrl-w" => Some(Key::CtrlW),
        "ctrl-x" => Some(Key::CtrlX),
        "ctrl-y" => Some(Key::CtrlY),
        "ctrl-z" => Some(Key::CtrlZ),

        "esc"                => Some(Key::ESC),
        "mouse"              => Some(Key::Mouse),
        "doubleclick"        => Some(Key::DoubleClick),
        "btab" | "shift-tab" => Some(Key::BTab),
        "bspace" | "bs"      => Some(Key::BSpace),
        "del"                => Some(Key::Del),
        "pgup" | "page-up"   => Some(Key::PgUp),
        "pgdn" | "page-down" => Some(Key::PgDn),
        "up"                 => Some(Key::Up),
        "down"               => Some(Key::Down),
        "left"               => Some(Key::Left),
        "right"              => Some(Key::Right),
        "home"               => Some(Key::Home),
        "end"                => Some(Key::End),
        "shift-left"         => Some(Key::SLeft),
        "shift-right"        => Some(Key::SRight),

        "f1"  => Some(Key::F1),
        "f2"  => Some(Key::F2),
        "f3"  => Some(Key::F3),
        "f4"  => Some(Key::F4),
        "f5"  => Some(Key::F5),
        "f6"  => Some(Key::F6),
        "f7"  => Some(Key::F7),
        "f8"  => Some(Key::F8),
        "f9"  => Some(Key::F9),
        "f10" => Some(Key::F10),
        "f11" => Some(Key::F11),
        "f12" => Some(Key::F12),

        "altenter"              => Some(Key::AltEnter),
        "altspace"              => Some(Key::AltSpace),
        "altslash"              => Some(Key::AltSlash),
        "alt-bs" | "alt-bspace" => Some(Key::AltBS),

        "alt-a" => Some(Key::AltA),
        "alt-b" => Some(Key::AltB),
        "alt-c" => Some(Key::AltC),
        "alt-d" => Some(Key::AltD),
        "alt-e" => Some(Key::AltE),
        "alt-f" => Some(Key::AltF),
        "alt-g" => Some(Key::AltG),
        "alt-h" => Some(Key::AltH),
        "alt-i" => Some(Key::AltI),
        "alt-j" => Some(Key::AltJ),
        "alt-k" => Some(Key::AltK),
        "alt-l" => Some(Key::AltL),
        "alt-m" => Some(Key::AltM),
        "alt-n" => Some(Key::AltN),
        "alt-o" => Some(Key::AltO),
        "alt-p" => Some(Key::AltP),
        "alt-q" => Some(Key::AltQ),
        "alt-r" => Some(Key::AltR),
        "alt-s" => Some(Key::AltS),
        "alt-t" => Some(Key::AltT),
        "alt-u" => Some(Key::AltU),
        "alt-v" => Some(Key::AltV),
        "alt-w" => Some(Key::AltW),
        "alt-x" => Some(Key::AltX),
        "alt-y" => Some(Key::AltY),
        "alt-z" => Some(Key::AltZ),
        ch if ch.chars().count() == 1 => Some(Key::Char(ch.chars().next().unwrap())),
        _ => None,
    }
}

fn get_default_key_map() -> HashMap<Key, (Event, Option<String>)> {
    let mut ret = HashMap::new();
    ret.insert(Key::ESC,   (Event::EvActAbort, None));
    ret.insert(Key::CtrlC, (Event::EvActAbort, None));
    ret.insert(Key::CtrlG, (Event::EvActAbort, None));

    ret.insert(Key::Enter, (Event::EvActAccept, None));

    ret.insert(Key::Left,  (Event::EvActBackwardChar, None));
    ret.insert(Key::CtrlB, (Event::EvActBackwardChar, None));

    ret.insert(Key::CtrlH, (Event::EvActBackwardDeleteChar, None));
    ret.insert(Key::BSpace,(Event::EvActBackwardDeleteChar, None));

    ret.insert(Key::AltBS, (Event::EvActBackwardKillWord, None));

    ret.insert(Key::AltB,  (Event::EvActBackwardWord, None));
    ret.insert(Key::SLeft, (Event::EvActBackwardWord, None));

    ret.insert(Key::CtrlA, (Event::EvActBeginningOfLine, None));
    //ret.insert(Key::AltB,  (Event::EvActCancel, None));
    ret.insert(Key::CtrlL, (Event::EvActClearScreen, None));
    ret.insert(Key::Del,   (Event::EvActDeleteChar, None));
    ret.insert(Key::CtrlD, (Event::EvActDeleteCharEOF, None));
    //ret.insert(Key::AltZ,  (Event::EvActDeselectAll, None));

    ret.insert(Key::CtrlJ, (Event::EvActDown, None));
    ret.insert(Key::CtrlN, (Event::EvActDown, None));
    ret.insert(Key::Down,  (Event::EvActDown, None));

    ret.insert(Key::CtrlE, (Event::EvActEndOfLine, None));
    ret.insert(Key::End,   (Event::EvActEndOfLine, None));

    ret.insert(Key::CtrlF, (Event::EvActForwardChar, None));
    ret.insert(Key::Right, (Event::EvActForwardChar, None));

    ret.insert(Key::AltF,  (Event::EvActForwardWord, None));
    ret.insert(Key::SRight,(Event::EvActForwardWord, None));

    //ret.insert(Key::AltZ,  (Event::EvActIgnore, None));

    ret.insert(Key::CtrlK, (Event::EvActKillLine, None));
    ret.insert(Key::AltD,  (Event::EvActKillWord, None));
    //ret.insert(Key::CtrlN, (Event::EvActNextHistory, None));
    ret.insert(Key::PgDn,  (Event::EvActPageDown, None));
    ret.insert(Key::PgUp,  (Event::EvActPageUp, None));
    ret.insert(Key::CtrlP, (Event::EvActPreviousHistory, None));
    ret.insert(Key::CtrlR, (Event::EvActRotateMode, None));
    ret.insert(Key::AltH,  (Event::EvActScrollLeft, None));
    ret.insert(Key::AltL,  (Event::EvActScrollRight, None));
    //ret.insert(Key::AltZ,  (Event::EvActSelectAll, None));
    //ret.insert(Key::AltZ,  (Event::EvActToggle, None));
    //ret.insert(Key::AltZ,  (Event::EvActToggleAll, None));
    ret.insert(Key::Tab,   (Event::EvActToggleDown, None));
    //ret.insert(Key::AltZ,  (Event::EvActToggleIn, None));
    ret.insert(Key::CtrlQ,  (Event::EvActToggleInteractive, None));
    //ret.insert(Key::AltZ,  (Event::EvActToggleOut, None));
    //ret.insert(Key::AltZ,  (Event::EvActToggleSort, None));
    ret.insert(Key::BTab,  (Event::EvActToggleUp, None));
    ret.insert(Key::CtrlU, (Event::EvActUnixLineDiscard, None));
    ret.insert(Key::CtrlW, (Event::EvActUnixWordRubout, None));
    ret.insert(Key::CtrlP, (Event::EvActUp, None));
    ret.insert(Key::CtrlK, (Event::EvActUp, None));
    ret.insert(Key::Up,    (Event::EvActUp, None));
    ret.insert(Key::CtrlY, (Event::EvActYank, None));
    ret
}

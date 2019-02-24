use crate::event::{parse_action, Event, EventSender};
/// Input will listens to user input, modify the query string, send special
/// keystrokes(such as Enter, Ctrl-p, Ctrl-n, etc) to the controller.
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs::File;
use std::io::prelude::*;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;
use tuikit::term::Term;
use tuikit::key::{Key, from_keyname};
use tuikit::event::Event as TuiEvent;

pub struct Input {
    tx_input: EventSender,
    term: Arc<Term>,
    keymap: HashMap<Key, (Event, Option<String>)>,
}

impl Input {
    pub fn new(term: Arc<Term>, tx_input: EventSender) -> Self {
        Input {
            tx_input,
            term,
            keymap: get_default_key_map(),
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.term.poll_event() {
                // search event from keymap
                Ok(TuiEvent::Key(key)) => match self.keymap.get(&key) {
                    Some(&(ev @ Event::EvActAccept, None)) | Some(&(ev @ Event::EvActAbort, None)) => {
                        let _ = self.tx_input.send((ev, Box::new(None as Option<String>)));
                        break;
                    }
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

                Ok(TuiEvent::Resize { width, height }) => {
                    let _ = self.tx_input.send((Event::EvActRedraw, Box::new(true)));
                }

                _ => {
                    let _ = self.tx_input.send((Event::EvInputInvalid, Box::new(true)));
                }
            }
        }
    }

    pub fn bind(&mut self, key: &str, action: &str, args: Option<String>) {
        let key = from_keyname(key);
        let action = parse_action(action);
        if key == None || action == None {
            return;
        }

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

fn get_default_key_map() -> HashMap<Key, (Event, Option<String>)> {
    let mut ret = HashMap::new();
    ret.insert(Key::ESC, (Event::EvActAbort, None));
    ret.insert(Key::Ctrl('C'), (Event::EvActAbort, None));
    ret.insert(Key::Char('G'), (Event::EvActAbort, None));

    ret.insert(Key::Enter, (Event::EvActAccept, None));

    ret.insert(Key::Left, (Event::EvActBackwardChar, None));
    ret.insert(Key::Ctrl('B'), (Event::EvActBackwardChar, None));

    ret.insert(Key::Char('H'), (Event::EvActBackwardDeleteChar, None));
    ret.insert(Key::Backspace, (Event::EvActBackwardDeleteChar, None));

    ret.insert(Key::AltBackspace, (Event::EvActBackwardKillWord, None));

    ret.insert(Key::Alt('B'), (Event::EvActBackwardWord, None));
    ret.insert(Key::ShiftLeft, (Event::EvActBackwardWord, None));

    ret.insert(Key::Ctrl('A'), (Event::EvActBeginningOfLine, None));
    //ret.insert(Key::Alt('B'),  (Event::EvActCancel, None));
    ret.insert(Key::Ctrl('L'), (Event::EvActClearScreen, None));
    ret.insert(Key::Del, (Event::EvActDeleteChar, None));
    ret.insert(Key::Ctrl('D'), (Event::EvActDeleteCharEOF, None));
    //ret.insert(Key::Alt('Z'),  (Event::EvActDeselectAll, None));

    ret.insert(Key::Ctrl('J'), (Event::EvActDown, None));
    ret.insert(Key::Ctrl('N'), (Event::EvActDown, None));
    ret.insert(Key::Down, (Event::EvActDown, None));

    ret.insert(Key::Ctrl('E'), (Event::EvActEndOfLine, None));
    ret.insert(Key::End, (Event::EvActEndOfLine, None));

    ret.insert(Key::Ctrl('F'), (Event::EvActForwardChar, None));
    ret.insert(Key::Right, (Event::EvActForwardChar, None));

    ret.insert(Key::Alt('F'), (Event::EvActForwardWord, None));
    ret.insert(Key::ShiftRight, (Event::EvActForwardWord, None));

    //ret.insert(Key::Alt('Z'),  (Event::EvActIgnore, None));

    ret.insert(Key::Alt('D'), (Event::EvActKillWord, None));
    //ret.insert(Key::Ctrl('N'), (Event::EvActNextHistory, None));
    ret.insert(Key::PageDown, (Event::EvActPageDown, None));
    ret.insert(Key::PageUp, (Event::EvActPageUp, None));
    ret.insert(Key::Ctrl('R'), (Event::EvActRotateMode, None));
    ret.insert(Key::Alt('H'), (Event::EvActScrollLeft, None));
    ret.insert(Key::Alt('L'), (Event::EvActScrollRight, None));
    //ret.insert(Key::Alt('Z'),  (Event::EvActSelectAll, None));
    //ret.insert(Key::Alt('Z'),  (Event::EvActToggle, None));
    //ret.insert(Key::Alt('Z'),  (Event::EvActToggleAll, None));
    ret.insert(Key::Tab, (Event::EvActToggleDown, None));
    //ret.insert(Key::Alt('Z'),  (Event::EvActToggleIn, None));
    ret.insert(Key::Ctrl('Q'), (Event::EvActToggleInteractive, None));
    //ret.insert(Key::Alt('Z'),  (Event::EvActToggleOut, None));
    //ret.insert(Key::Alt('Z'),  (Event::EvActToggleSort, None));
    ret.insert(Key::BackTab, (Event::EvActToggleUp, None));
    ret.insert(Key::Ctrl('U'), (Event::EvActUnixLineDiscard, None));
    ret.insert(Key::Ctrl('W'), (Event::EvActUnixWordRubout, None));
    ret.insert(Key::Ctrl('P'), (Event::EvActUp, None));
    ret.insert(Key::Ctrl('K'), (Event::EvActUp, None));
    ret.insert(Key::Up, (Event::EvActUp, None));
    ret.insert(Key::Ctrl('U'), (Event::EvActYank, None));
    ret
}

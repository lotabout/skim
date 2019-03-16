///! Input will listens to user input, modify the query string, send special
///! keystrokes(such as Enter, Ctrl-p, Ctrl-n, etc) to the controller.
use crate::event::{parse_action, Event, EventArg};
use std::collections::HashMap;
use std::sync::Arc;
use tuikit::event::Event as TuiEvent;
use tuikit::key::{from_keyname, Key};
use tuikit::term::Term;

pub struct Input {
    term: Arc<Term>,
    keymap: HashMap<Key, (Event, Option<String>)>,
}

impl Input {
    pub fn new(term: Arc<Term>) -> Self {
        Input {
            term,
            keymap: get_default_key_map(),
        }
    }

    pub fn pool_event(&self) -> (Event, EventArg) {
        match self.term.poll_event() {
            // search event from keymap
            Ok(TuiEvent::Key(key)) => match self.keymap.get(&key) {
                Some(&(ev, Some(ref args))) => (ev, Box::new(Some(args.clone()))),
                Some(&(ev, None)) => (ev, Box::new(None as Option<String>)),
                None => {
                    if let Key::Char(ch) = key {
                        (Event::EvActAddChar, Box::new(ch))
                    } else {
                        (Event::EvInputKey, Box::new(key))
                    }
                }
            },

            Ok(TuiEvent::Resize { .. }) => (Event::EvActRedraw, Box::new(true)),

            _ => (Event::EvInputInvalid, Box::new(true)),
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
    ret.insert(Key::Ctrl('c'), (Event::EvActAbort, None));
    ret.insert(Key::Ctrl('g'), (Event::EvActAbort, None));

    ret.insert(Key::Enter, (Event::EvActAccept, None));

    ret.insert(Key::Left, (Event::EvActBackwardChar, None));
    ret.insert(Key::Ctrl('b'), (Event::EvActBackwardChar, None));

    ret.insert(Key::Ctrl('h'), (Event::EvActBackwardDeleteChar, None));
    ret.insert(Key::Backspace, (Event::EvActBackwardDeleteChar, None));

    ret.insert(Key::AltBackspace, (Event::EvActBackwardKillWord, None));

    ret.insert(Key::Alt('b'), (Event::EvActBackwardWord, None));
    ret.insert(Key::ShiftLeft, (Event::EvActBackwardWord, None));

    ret.insert(Key::Ctrl('a'), (Event::EvActBeginningOfLine, None));
    //ret.insert(Key::Alt('b'),  (Event::EvActCancel, None));
    ret.insert(Key::Ctrl('l'), (Event::EvActClearScreen, None));
    ret.insert(Key::Del, (Event::EvActDeleteChar, None));
    ret.insert(Key::Ctrl('d'), (Event::EvActDeleteCharEOF, None));
    //ret.insert(Key::Alt('z'),  (Event::EvActDeselectAll, None));

    ret.insert(Key::Ctrl('j'), (Event::EvActDown, None));
    ret.insert(Key::Ctrl('n'), (Event::EvActDown, None));
    ret.insert(Key::Down, (Event::EvActDown, None));

    ret.insert(Key::Ctrl('e'), (Event::EvActEndOfLine, None));
    ret.insert(Key::End, (Event::EvActEndOfLine, None));

    ret.insert(Key::Ctrl('f'), (Event::EvActForwardChar, None));
    ret.insert(Key::Right, (Event::EvActForwardChar, None));

    ret.insert(Key::Alt('f'), (Event::EvActForwardWord, None));
    ret.insert(Key::ShiftRight, (Event::EvActForwardWord, None));

    //ret.insert(Key::Alt('z'),  (Event::EvActIgnore, None));

    ret.insert(Key::Alt('d'), (Event::EvActKillWord, None));
    //ret.insert(Key::Ctrl('n'), (Event::EvActNextHistory, None));
    ret.insert(Key::PageDown, (Event::EvActPageDown, None));
    ret.insert(Key::PageUp, (Event::EvActPageUp, None));
    ret.insert(Key::Ctrl('r'), (Event::EvActRotateMode, None));
    ret.insert(Key::Alt('h'), (Event::EvActScrollLeft, None));
    ret.insert(Key::Alt('l'), (Event::EvActScrollRight, None));
    //ret.insert(Key::Alt('z'),  (Event::EvActSelectAll, None));
    //ret.insert(Key::Alt('z'),  (Event::EvActToggle, None));
    //ret.insert(Key::Alt('z'),  (Event::EvActToggleAll, None));
    ret.insert(Key::Tab, (Event::EvActToggleDown, None));
    //ret.insert(Key::Alt('z'),  (Event::EvActToggleIn, None));
    ret.insert(Key::Ctrl('q'), (Event::EvActToggleInteractive, None));
    //ret.insert(Key::Alt('z'),  (Event::EvActToggleOut, None));
    //ret.insert(Key::Alt('z'),  (Event::EvActToggleSort, None));
    ret.insert(Key::BackTab, (Event::EvActToggleUp, None));
    ret.insert(Key::Ctrl('u'), (Event::EvActUnixLineDiscard, None));
    ret.insert(Key::Ctrl('w'), (Event::EvActUnixWordRubout, None));
    ret.insert(Key::Ctrl('p'), (Event::EvActUp, None));
    ret.insert(Key::Ctrl('k'), (Event::EvActUp, None));
    ret.insert(Key::Up, (Event::EvActUp, None));
    ret.insert(Key::Ctrl('y'), (Event::EvActYank, None));
    ret
}

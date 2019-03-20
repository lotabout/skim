use std::collections::HashMap;
use std::sync::Arc;

use regex::Regex;
use tuikit::event::Event as TuiEvent;
use tuikit::key::{from_keyname, Key};
use tuikit::term::Term;

///! Input will listens to user input, modify the query string, send special
///! keystrokes(such as Enter, Ctrl-p, Ctrl-n, etc) to the controller.
use crate::event::{parse_action, Event, EventArg};

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
        for (key, action, args) in parse_key_action(key_action).into_iter() {
            self.bind(key, action, args);
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

/// parse key action string to `(key, action, argument)` tuple
/// key_action is comma separated: 'ctrl-j:accept,ctrl-k:kill-line'
fn parse_key_action(key_action: &str) -> Vec<(&str, &str, Option<String>)> {
    lazy_static! {
        // match `key:action` or `key:action:arg` or `key:action(arg)` etc.
        static ref RE: Regex =
            Regex::new(r#"(?si)[^:]+?:[a-z-]+?\s*(?:"[^"]*?"|'[^']*?'|\([^\)]*?\)|\[[^\]]*?\]|:[^:]*?)?\s*(,|$)"#)
                .unwrap();
        // grab key, action and arg out.
        static ref RE_BIND: Regex = Regex::new(r#"(?si)([^:]+?):([a-z-]+)(?:[:\(\["'](.+?)[\)"'\]]?)?,?$"#).unwrap();
    }

    RE.find_iter(&key_action)
        .map(|mat| {
            let caps = RE_BIND.captures(mat.as_str()).unwrap();
            (
                caps.get(1).unwrap().as_str(),
                caps.get(2).unwrap().as_str(),
                caps.get(3).map(|s| s.as_str().to_string()),
            )
        })
        .collect()
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
    ret.insert(Key::Null, (Event::EvActAbort, None));
    ret
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn execute_should_be_parsed_correctly() {
        // example from https://github.com/lotabout/skim/issues/73
        let cmd = "
        (grep -o '[a-f0-9]\\{7\\}' | head -1 |
        xargs -I % sh -c 'git show --color=always % | less -R') << 'FZF-EOF'
        {}
        FZF-EOF";

        let key_action_str = format!("ctrl-s:toggle-sort,ctrl-m:execute:{},ctrl-t:toggle", cmd);

        let key_action = parse_key_action(&key_action_str);
        assert_eq!(("ctrl-s", "toggle-sort", None), key_action[0]);
        assert_eq!(("ctrl-m", "execute", Some(cmd.to_string())), key_action[1]);
        assert_eq!(("ctrl-t", "toggle", None), key_action[2]);

        let key_action_str = "f1:execute(less -f {}),ctrl-y:execute-silent(echo {} | pbcopy)";
        let key_action = parse_key_action(&key_action_str);
        assert_eq!(("f1", "execute", Some("less -f {}".to_string())), key_action[0]);
        assert_eq!(("ctrl-y", "execute-silent", Some("echo {} | pbcopy".to_string())), key_action[1]);
    }

}

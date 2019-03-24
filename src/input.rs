///! Input will listens to user input, modify the query string, send special
///! keystrokes(such as Enter, Ctrl-p, Ctrl-n, etc) to the controller.
use crate::event::{parse_action, Event, EventArg};
use regex::Regex;
use std::collections::HashMap;
use tuikit::event::Event as TuiEvent;
use tuikit::key::{from_keyname, Key};

#[derive(Debug, Clone, PartialEq)]
pub enum ActionArg {
    Char(char),
    String(String),
    None,
}

pub type Action = (Event, ActionArg);
pub type ActionChain = Vec<Action>;

pub struct Input {
    keymap: HashMap<Key, ActionChain>,
}

impl From<ActionArg> for EventArg {
    fn from(arg: ActionArg) -> Self {
        match arg {
            ActionArg::Char(ch) => Box::new(ch),
            ActionArg::String(string) => Box::new(Some(string)),
            ActionArg::None => Box::new(None as Option<String>),
        }
    }
}

impl Input {
    pub fn new() -> Self {
        Input {
            keymap: get_default_key_map(),
        }
    }

    pub fn translate_event(&self, event: TuiEvent) -> Vec<(Event, EventArg)> {
        match event {
            // search event from keymap
            TuiEvent::Key(key) => self
                .keymap
                .get(&key)
                .map(|chain| {
                    chain
                        .iter()
                        .map(|(action, arg)| (action.clone(), arg.clone().into()))
                        .collect()
                })
                .unwrap_or_else(|| {
                    if let Key::Char(ch) = key {
                        vec![(Event::EvActAddChar, Box::new(ch) as EventArg)]
                    } else {
                        vec![(Event::EvInputKey, Box::new(key) as EventArg)]
                    }
                }),
            TuiEvent::Resize { .. } => vec![(Event::EvActRedraw, Box::new(true))],
            _ => vec![(Event::EvInputInvalid, Box::new(true))],
        }
    }

    pub fn bind(&mut self, key: &str, action_chain: ActionChain) {
        let key = from_keyname(key);
        if key == None || action_chain.is_empty() {
            return;
        }

        let key = key.unwrap();

        // remove the key for existing keymap;
        let _ = self.keymap.remove(&key);
        self.keymap.entry(key).or_insert(action_chain);
    }

    pub fn parse_keymaps(&mut self, maps: &[&str]) {
        for &map in maps {
            self.parse_keymap(map);
        }
    }

    // key_action is comma separated: 'ctrl-j:accept,ctrl-k:kill-line'
    pub fn parse_keymap(&mut self, key_action: &str) {
        for (key, action_chain) in parse_key_action(key_action).into_iter() {
            let action_chain = action_chain
                .into_iter()
                .filter_map(|(action, arg)| {
                    parse_action(action).map(|act| (act, arg.map(ActionArg::String).unwrap_or(ActionArg::None)))
                })
                .collect();
            self.bind(key, action_chain);
        }
    }

    pub fn parse_expect_keys(&mut self, keys: Option<&str>) {
        if let Some(keys) = keys {
            self.bind("enter", vec![(Event::EvActAccept, ActionArg::String("".to_string()))]);
            for key in keys.split(',') {
                self.bind(key, vec![(Event::EvActAccept, ActionArg::String(key.to_string()))]);
            }
        }
    }
}

/// parse key action string to `(key, action, argument)` tuple
/// key_action is comma separated: 'ctrl-j:accept,ctrl-k:kill-line'
fn parse_key_action(key_action: &str) -> Vec<(&str, Vec<(&str, Option<String>)>)> {
    lazy_static! {
        // match `key:action` or `key:action:arg` or `key:action(arg)` etc.
        static ref RE: Regex =
            Regex::new(r#"(?si)([^:]+?):((?:\+?[a-z-]+?(?:"[^"]*?"|'[^']*?'|\([^\)]*?\)|\[[^\]]*?\]|:[^:]*?)?\s*)+)(?:,|$)"#)
                .unwrap();
        // grab key, action and arg out.
        static ref RE_BIND: Regex = Regex::new(r#"(?si)([a-z-]+)(?:[:\(\["'](.+?)[\)"'\]]?)?(?:\+|$)"#).unwrap();
    }

    RE.captures_iter(&key_action)
        .map(|caps| {
            let key = caps.get(1).unwrap().as_str();
            let actions = RE_BIND
                .captures_iter(caps.get(2).unwrap().as_str())
                .map(|caps| {
                    (
                        caps.get(1).unwrap().as_str(),
                        caps.get(2).map(|s| s.as_str().to_string()),
                    )
                })
                .collect();
            (key, actions)
        })
        .collect()
}

#[rustfmt::skip]
fn get_default_key_map() -> HashMap<Key, ActionChain> {
    use self::ActionArg::*;
    let mut ret = HashMap::new();
    ret.insert(Key::ESC,          vec![(Event::EvActAbort,              None)]);
    ret.insert(Key::Ctrl('c'),    vec![(Event::EvActAbort,              None)]);
    ret.insert(Key::Ctrl('g'),    vec![(Event::EvActAbort,              None)]);
    ret.insert(Key::Enter,        vec![(Event::EvActAccept,             None)]);
    ret.insert(Key::Left,         vec![(Event::EvActBackwardChar,       None)]);
    ret.insert(Key::Ctrl('b'),    vec![(Event::EvActBackwardChar,       None)]);
    ret.insert(Key::Ctrl('h'),    vec![(Event::EvActBackwardDeleteChar, None)]);
    ret.insert(Key::Backspace,    vec![(Event::EvActBackwardDeleteChar, None)]);
    ret.insert(Key::AltBackspace, vec![(Event::EvActBackwardKillWord,   None)]);
    ret.insert(Key::Alt('b'),     vec![(Event::EvActBackwardWord,       None)]);
    ret.insert(Key::ShiftLeft,    vec![(Event::EvActBackwardWord,       None)]);
    ret.insert(Key::Ctrl('a'),    vec![(Event::EvActBeginningOfLine,    None)]);
    ret.insert(Key::Ctrl('l'),    vec![(Event::EvActClearScreen,        None)]);
    ret.insert(Key::Delete,       vec![(Event::EvActDeleteChar,         None)]);
    ret.insert(Key::Ctrl('d'),    vec![(Event::EvActDeleteCharEOF,      None)]);
    ret.insert(Key::Ctrl('j'),    vec![(Event::EvActDown,               None)]);
    ret.insert(Key::Ctrl('n'),    vec![(Event::EvActDown,               None)]);
    ret.insert(Key::Down,         vec![(Event::EvActDown,               None)]);
    ret.insert(Key::Ctrl('e'),    vec![(Event::EvActEndOfLine,          None)]);
    ret.insert(Key::End,          vec![(Event::EvActEndOfLine,          None)]);
    ret.insert(Key::Ctrl('f'),    vec![(Event::EvActForwardChar,        None)]);
    ret.insert(Key::Right,        vec![(Event::EvActForwardChar,        None)]);
    ret.insert(Key::Alt('f'),     vec![(Event::EvActForwardWord,        None)]);
    ret.insert(Key::ShiftRight,   vec![(Event::EvActForwardWord,        None)]);
    ret.insert(Key::Alt('d'),     vec![(Event::EvActKillWord,           None)]);
    ret.insert(Key::ShiftUp,      vec![(Event::EvActPreviewPageUp,      None)]);
    ret.insert(Key::ShiftDown,    vec![(Event::EvActPreviewPageDown,    None)]);
    ret.insert(Key::PageDown,     vec![(Event::EvActPageDown,           None)]);
    ret.insert(Key::PageUp,       vec![(Event::EvActPageUp,             None)]);
    ret.insert(Key::Ctrl('r'),    vec![(Event::EvActRotateMode,         None)]);
    ret.insert(Key::Alt('h'),     vec![(Event::EvActScrollLeft,         None)]);
    ret.insert(Key::Alt('l'),     vec![(Event::EvActScrollRight,        None)]);
    ret.insert(Key::Tab,          vec![(Event::EvActToggle,             None), (Event::EvActDown, None)]);
    ret.insert(Key::Ctrl('q'),    vec![(Event::EvActToggleInteractive,  None)]);
    ret.insert(Key::BackTab,      vec![(Event::EvActToggle,             None), (Event::EvActUp,   None)]);
    ret.insert(Key::Ctrl('u'),    vec![(Event::EvActUnixLineDiscard,    None)]);
    ret.insert(Key::Ctrl('w'),    vec![(Event::EvActUnixWordRubout,     None)]);
    ret.insert(Key::Ctrl('p'),    vec![(Event::EvActUp,                 None)]);
    ret.insert(Key::Ctrl('k'),    vec![(Event::EvActUp,                 None)]);
    ret.insert(Key::Up,           vec![(Event::EvActUp,                 None)]);
    ret.insert(Key::Ctrl('y'),    vec![(Event::EvActYank,               None)]);
    ret.insert(Key::Null,         vec![(Event::EvActAbort,              None)]);
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
        assert_eq!(("ctrl-s", vec![("toggle-sort", None)]), key_action[0]);
        assert_eq!(("ctrl-m", vec![("execute", Some(cmd.to_string()))]), key_action[1]);
        assert_eq!(("ctrl-t", vec![("toggle", None)]), key_action[2]);

        let key_action_str = "f1:execute(less -f {}),ctrl-y:execute-silent(echo {} | pbcopy)";
        let key_action = parse_key_action(&key_action_str);
        assert_eq!(("f1", vec![("execute", Some("less -f {}".to_string()))]), key_action[0]);
        assert_eq!(
            ("ctrl-y", vec![("execute-silent", Some("echo {} | pbcopy".to_string()))]),
            key_action[1]
        );
    }

    #[test]
    fn action_chain_should_be_parsed() {
        let key_action = parse_key_action("ctrl-t:toggle+up");
        assert_eq!(("ctrl-t", vec![("toggle", None), ("up", None)]), key_action[0]);

        let key_action_str = "f1:execute(less -f {}),ctrl-y:execute-silent(echo {} | pbcopy)+abort";
        let key_action = parse_key_action(&key_action_str);
        assert_eq!(("f1", vec![("execute", Some("less -f {}".to_string()))]), key_action[0]);
        assert_eq!(
            (
                "ctrl-y",
                vec![
                    ("execute-silent", Some("echo {} | pbcopy".to_string())),
                    ("abort", None)
                ]
            ),
            key_action[1]
        );
    }
}

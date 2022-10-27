///! Input will listens to user input, modify the query string, send special
///! keystrokes(such as Enter, Ctrl-p, Ctrl-n, etc) to the controller.
use crate::event::{parse_event, Event};
use regex::Regex;
use std::collections::HashMap;
use tuikit::event::Event as TermEvent;
use tuikit::key::{from_keyname, Key};

pub type ActionChain = Vec<Event>;

pub struct Input {
    keymap: HashMap<Key, ActionChain>,
}

impl Input {
    pub fn new() -> Self {
        Input {
            keymap: get_default_key_map(),
        }
    }

    pub fn translate_event(&self, event: TermEvent) -> (Key, ActionChain) {
        match event {
            // search event from keymap
            TermEvent::Key(key) => (
                key,
                self.keymap.get(&key).cloned().unwrap_or_else(|| {
                    if let Key::Char(ch) = key {
                        vec![Event::EvActAddChar(ch)]
                    } else {
                        vec![Event::EvInputKey(key)]
                    }
                }),
            ),
            TermEvent::Resize { .. } => (Key::Null, vec![Event::EvActRedraw]),
            _ => (Key::Null, vec![Event::EvInputInvalid]),
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
        debug!("got key_action: {:?}", key_action);
        for (key, action_chain) in parse_key_action(key_action).into_iter() {
            debug!("parsed key_action: {:?}: {:?}", key, action_chain);
            let action_chain = action_chain
                .into_iter()
                .filter_map(|(action, arg)| parse_event(action, arg))
                .collect();
            self.bind(key, action_chain);
        }
    }

    pub fn parse_expect_keys(&mut self, keys: Option<&str>) {
        if let Some(keys) = keys {
            for key in keys.split(',') {
                self.bind(key, vec![Event::EvActAccept(Some(key.to_string()))]);
            }
        }
    }
}

type KeyActions<'a> = (&'a str, Vec<(&'a str, Option<String>)>);

/// parse key action string to `(key, action, argument)` tuple
/// key_action is comma separated: 'ctrl-j:accept,ctrl-k:kill-line'
pub fn parse_key_action(key_action: &str) -> Vec<KeyActions> {
    lazy_static! {
        // match `key:action` or `key:action:arg` or `key:action(arg)` etc.
        static ref RE: Regex =
            Regex::new(r#"(?si)([^:]+?):((?:\+?[a-z-]+?(?:"[^"]*?"|'[^']*?'|\([^\)]*?\)|\[[^\]]*?\]|:[^:]*?)?\s*)+)(?:,|$)"#)
                .unwrap();
        // grab key, action and arg out.
        static ref RE_BIND: Regex = Regex::new(r#"(?si)([a-z-]+)("[^"]+?"|'[^']+?'|\([^\)]+?\)|\[[^\]]+?\]|:[^:]+?)?(?:\+|$)"#).unwrap();
    }

    RE.captures_iter(key_action)
        .map(|caps| {
            debug!("RE: caps: {:?}", caps);
            let key = caps.get(1).unwrap().as_str();
            let actions = RE_BIND
                .captures_iter(caps.get(2).unwrap().as_str())
                .map(|caps| {
                    debug!("RE_BIND: caps: {:?}", caps);
                    (
                        caps.get(1).unwrap().as_str(),
                        caps.get(2).map(|s| {
                            // (arg) => arg, :end_arg => arg
                            let action = s.as_str();
                            if let Some(stripped) = action.strip_prefix(':') {
                                stripped.to_owned()
                            } else {
                                action[1..action.len() - 1].to_string()
                            }
                        }),
                    )
                })
                .collect();
            (key, actions)
        })
        .collect()
}

/// e.g. execute(...) => Some(Event::EvActExecute, Box::new(Option("...")))
pub fn parse_action_arg(action_arg: &str) -> Option<Event> {
    // construct a fake key_action: `fake_key:action(arg)`
    let fake_key_action = format!("fake_key:{}", action_arg);
    // get keys: [(key, [(action, arg), (action, arg)]), ...]
    let keys = parse_key_action(&fake_key_action);
    // only get the first key(since it is faked), and get the first action
    if keys.is_empty() || keys[0].1.is_empty() {
        None
    } else {
        // first action pair of key(keys[0].1) and first action (keys[0].1[0])
        let (action, new_arg) = keys[0].1[0].clone();
        parse_event(action, new_arg)
    }
}

#[rustfmt::skip]
fn get_default_key_map() -> HashMap<Key, ActionChain> {
    let mut ret = HashMap::new();
    ret.insert(Key::ESC,          vec![Event::EvActAbort]);
    ret.insert(Key::Ctrl('c'),    vec![Event::EvActAbort]);
    ret.insert(Key::Ctrl('g'),    vec![Event::EvActAbort]);
    ret.insert(Key::Enter,        vec![Event::EvActAccept(None)]);
    ret.insert(Key::Left,         vec![Event::EvActBackwardChar]);
    ret.insert(Key::Ctrl('b'),    vec![Event::EvActBackwardChar]);
    ret.insert(Key::Ctrl('h'),    vec![Event::EvActBackwardDeleteChar]);
    ret.insert(Key::Backspace,    vec![Event::EvActBackwardDeleteChar]);
    ret.insert(Key::AltBackspace, vec![Event::EvActBackwardKillWord]);
    ret.insert(Key::Alt('b'),     vec![Event::EvActBackwardWord]);
    ret.insert(Key::ShiftLeft,    vec![Event::EvActBackwardWord]);
    ret.insert(Key::CtrlLeft,     vec![Event::EvActBackwardWord]);
    ret.insert(Key::Ctrl('a'),    vec![Event::EvActBeginningOfLine]);
    ret.insert(Key::Home,         vec![Event::EvActBeginningOfLine]);
    ret.insert(Key::Ctrl('l'),    vec![Event::EvActClearScreen]);
    ret.insert(Key::Delete,       vec![Event::EvActDeleteChar]);
    ret.insert(Key::Ctrl('d'),    vec![Event::EvActDeleteCharEOF]);
    ret.insert(Key::Ctrl('j'),    vec![Event::EvActDown(1)]);
    ret.insert(Key::Ctrl('n'),    vec![Event::EvActDown(1)]);
    ret.insert(Key::Down,         vec![Event::EvActDown(1)]);
    ret.insert(Key::Ctrl('e'),    vec![Event::EvActEndOfLine]);
    ret.insert(Key::End,          vec![Event::EvActEndOfLine]);
    ret.insert(Key::Ctrl('f'),    vec![Event::EvActForwardChar]);
    ret.insert(Key::Right,        vec![Event::EvActForwardChar]);
    ret.insert(Key::Alt('f'),     vec![Event::EvActForwardWord]);
    ret.insert(Key::CtrlRight,    vec![Event::EvActForwardWord]);
    ret.insert(Key::ShiftRight,   vec![Event::EvActForwardWord]);
    ret.insert(Key::Alt('d'),     vec![Event::EvActKillWord]);
    ret.insert(Key::ShiftUp,      vec![Event::EvActPreviewPageUp(1)]);
    ret.insert(Key::ShiftDown,    vec![Event::EvActPreviewPageDown(1)]);
    ret.insert(Key::PageDown,     vec![Event::EvActPageDown(1)]);
    ret.insert(Key::PageUp,       vec![Event::EvActPageUp(1)]);
    ret.insert(Key::Ctrl('r'),    vec![Event::EvActRotateMode]);
    ret.insert(Key::Alt('h'),     vec![Event::EvActScrollLeft(1)]);
    ret.insert(Key::Alt('l'),     vec![Event::EvActScrollRight(1)]);
    ret.insert(Key::Tab,          vec![Event::EvActToggle, Event::EvActDown(1)]);
    ret.insert(Key::Ctrl('q'),    vec![Event::EvActToggleInteractive]);
    ret.insert(Key::BackTab,      vec![Event::EvActToggle, Event::EvActUp(1)]);
    ret.insert(Key::Ctrl('u'),    vec![Event::EvActUnixLineDiscard]);
    ret.insert(Key::Ctrl('w'),    vec![Event::EvActUnixWordRubout]);
    ret.insert(Key::Ctrl('p'),    vec![Event::EvActUp(1)]);
    ret.insert(Key::Ctrl('k'),    vec![Event::EvActUp(1)]);
    ret.insert(Key::Up,           vec![Event::EvActUp(1)]);
    ret.insert(Key::Ctrl('y'),    vec![Event::EvActYank]);
    ret.insert(Key::Null,         vec![Event::EvActAbort]);
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
        let key_action = parse_key_action(key_action_str);
        assert_eq!(("f1", vec![("execute", Some("less -f {}".to_string()))]), key_action[0]);
        assert_eq!(
            ("ctrl-y", vec![("execute-silent", Some("echo {} | pbcopy".to_string()))]),
            key_action[1]
        );

        // #196
        let key_action_str = "enter:execute($EDITOR +{2} {1})";
        let key_action = parse_key_action(key_action_str);
        assert_eq!(
            ("enter", vec![("execute", Some("$EDITOR +{2} {1}".to_string()))]),
            key_action[0]
        );
    }

    #[test]
    fn action_chain_should_be_parsed() {
        let key_action = parse_key_action("ctrl-t:toggle+up");
        assert_eq!(("ctrl-t", vec![("toggle", None), ("up", None)]), key_action[0]);

        let key_action_str = "f1:execute(less -f {}),ctrl-y:execute-silent(echo {} | pbcopy)+abort";
        let key_action = parse_key_action(key_action_str);
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

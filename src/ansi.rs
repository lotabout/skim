// Parse ANSI attr code
use std::borrow::Cow;
use std::default::Default;
use std::mem;
use tuikit::attr::{Attr, Color, Effect};
use vte::Perform;

/// An ANSI Parser, will parse one line at a time.
///
/// It will cache the latest attribute used, that means if an attribute affect multiple
/// lines, the parser will recognize it.
pub struct ANSIParser {
    partial_str: String,
    last_attr: Attr,

    stripped: String,
    fragments: Vec<(Attr, Cow<'static, str>)>,
}

impl Default for ANSIParser {
    fn default() -> Self {
        ANSIParser {
            partial_str: String::new(),
            last_attr: Attr::default(),

            stripped: String::new(),
            fragments: Vec::new(),
        }
    }
}

impl Perform for ANSIParser {
    fn print(&mut self, ch: char) {
        self.partial_str.push(ch);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            // put back \0 \r \n \t
            0x00 | 0x0d | 0x0A | 0x09 => self.partial_str.push(byte as char),
            // ignore all others
            _ => trace!("AnsiParser:execute ignored {:?}", byte),
        }
    }

    fn hook(&mut self, params: &[i64], _intermediates: &[u8], _ignore: bool) {
        trace!("AnsiParser:hook ignored {:?}", params);
    }

    fn put(&mut self, byte: u8) {
        trace!("AnsiParser:put ignored {:?}", byte);
    }

    fn unhook(&mut self) {
        trace!("AnsiParser:unhook ignored");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]]) {
        trace!("AnsiParser:osc ignored {:?}", params);
    }

    fn csi_dispatch(&mut self, params: &[i64], _intermediates: &[u8], _ignore: bool, mode: char) {
        // https://en.wikipedia.org/wiki/ANSI_escape_code#SGR_(Select_Graphic_Rendition)_parameters
        // Only care about graphic modes, ignore all others

        if mode != 'm' {
            trace!("ignore: params: {:?}, mode: {:?}", params, mode);
            return;
        }

        let mut attr = self.last_attr;
        match params.len() {
            // CSI m, reset
            0 => attr = Attr::default(),
            // CSI <num> m
            1 => match params[0] {
                0 => attr = Attr::default(),
                1 => attr.effect |= Effect::BOLD,
                4 => attr.effect |= Effect::UNDERLINE,
                5 => attr.effect |= Effect::BLINK,
                7 => attr.effect |= Effect::REVERSE,
                num if num >= 30 && num <= 37 => {
                    attr.fg = Color::AnsiValue((num - 30) as u8);
                }
                num if num >= 40 && num <= 47 => {
                    attr.bg = Color::AnsiValue((num - 40) as u8);
                }
                39 => attr.fg = Color::Default,
                49 => attr.bg = Color::Default,
                _ => {
                    trace!("ignore CSI {:?} m", params);
                }
            },
            // ESC[ 38;5;<n> m Select foreground color
            // ESC[ 48;5;<n> m Select background color
            3 => {
                if params[1] != 5 {
                    trace!("ignore CSI {:?} m", params);
                } else {
                    let color = Color::AnsiValue(params[2] as u8);
                    if params[0] == 38 {
                        attr.fg = color;
                    } else if params[0] == 48 {
                        attr.bg = color;
                    }
                }
            }
            // ESC[ 38;2;<r>;<g>;<b> m Select RGB foreground color
            // ESC[ 48;2;<r>;<g>;<b> m Select RGB background color
            5 => {
                if params[1] != 2 {
                    trace!("ignore CSI {:?} m", params);
                } else {
                    let r = params[2] as u8;
                    let g = params[3] as u8;
                    let b = params[4] as u8;
                    let color = Color::Rgb(r, g, b);
                    if params[0] == 38 {
                        attr.fg = color;
                    } else if params[0] == 48 {
                        attr.bg = color;
                    }
                }
            }
            _ => trace!("ignore CSI {:?} m", params),
        }

        self.attr_change(attr);
    }

    fn esc_dispatch(&mut self, _params: &[i64], _intermediates: &[u8], _ignore: bool, _byte: u8) {
        // ESC characters are replaced with \[
        self.partial_str.push('"');
        self.partial_str.push('[');
    }
}

impl ANSIParser {
    /// save the partial_str into fragments with current attr
    fn save_str(&mut self) {
        if self.partial_str.is_empty() {
            return;
        }

        let string = mem::replace(&mut self.partial_str, String::new());
        self.stripped.push_str(&string);
        self.fragments.push((self.last_attr, Cow::Owned(string)));
    }

    // accept a new attr
    fn attr_change(&mut self, new_attr: Attr) {
        if new_attr == self.last_attr {
            return;
        }

        self.save_str();
        self.last_attr = new_attr;
    }

    pub fn parse_ansi(&mut self, text: &str) -> AnsiString {
        let mut statemachine = vte::Parser::new();

        for byte in text.as_bytes() {
            statemachine.advance(self, *byte);
        }
        self.save_str();

        let stripped = mem::replace(&mut self.stripped, String::new());
        let fragments = mem::replace(&mut self.fragments, Vec::new());
        AnsiString::new(stripped, fragments)
    }
}

#[derive(Clone, Debug)]
/// A String that contains ANSI state (e.g. colors)
///
/// It is internally represented as Vec<(attr, string)>
pub struct AnsiString {
    stripped: Cow<'static, str>,
    fragments: Vec<(Attr, Cow<'static, str>)>,
}

impl AnsiString {
    pub fn new_empty() -> Self {
        Self {
            stripped: Cow::Owned(String::new()),
            fragments: Vec::new(),
        }
    }

    pub fn new_string(string: String) -> Self {
        let stripped: Cow<'static, str> = Cow::Owned(string);
        Self {
            stripped: stripped.clone(),
            fragments: vec![(Attr::default(), stripped.clone())],
        }
    }

    pub fn new(stripped: String, fragments: Vec<(Attr, Cow<'static, str>)>) -> Self {
        Self {
            stripped: Cow::Owned(stripped),
            fragments,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.fragments.is_empty()
    }

    pub fn into_inner(self) -> String {
        self.stripped.into()
    }

    pub fn iter(&self) -> AnsiStringIterator {
        AnsiStringIterator::new(&self.fragments)
    }

    pub fn has_attrs(&self) -> bool {
        self.fragments.len() > 1
    }

    pub fn from_str(raw: &str) -> AnsiString {
        ANSIParser::default().parse_ansi(raw)
    }

    pub fn get_stripped(&self) -> &str {
        &self.stripped
    }
}

/// An iterator over all the (char, attr) characters.
pub struct AnsiStringIterator<'a> {
    fragments: &'a [(Attr, Cow<'a, str>)],
    fragment_idx: usize,
    attr: Attr,
    chars_iter: Option<std::str::Chars<'a>>,
}

impl<'a> AnsiStringIterator<'a> {
    pub fn new(fragments: &'a [(Attr, Cow<'a, str>)]) -> Self {
        Self {
            fragments,
            fragment_idx: 0,
            attr: Attr::default(),
            chars_iter: None,
        }
    }
}

impl<'a> Iterator for AnsiStringIterator<'a> {
    type Item = (char, Attr);

    fn next(&mut self) -> Option<Self::Item> {
        let ch = self.chars_iter.as_mut().and_then(|iter| iter.next());
        if ch.is_some() {
            Some((ch.unwrap(), self.attr))
        } else if ch.is_none() && self.fragment_idx >= self.fragments.len() {
            None
        } else {
            // try next fragment
            let (attr, string) = &self.fragments[self.fragment_idx];
            self.attr = *attr;
            self.chars_iter.replace(string.chars());
            self.fragment_idx += 1;
            self.next()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ansi_iterator() {
        let input = "\x1B[48;2;5;10;15m\x1B[38;2;70;130;180mhi\x1B[0m";
        let ansistring = ANSIParser::default().parse_ansi(input);
        let mut it = ansistring.iter();
        let attr = Attr {
            fg: Color::Rgb(70, 130, 180),
            bg: Color::Rgb(5, 10, 15),
            ..Attr::default()
        };

        assert_eq!(Some(('h', attr)), it.next());
        assert_eq!(Some(('i', attr)), it.next());
        assert_eq!(None, it.next());
    }

    #[test]
    fn test_normal_string() {
        let input = "ab";
        let ansistring = ANSIParser::default().parse_ansi(input);

        assert_eq!(false, ansistring.has_attrs());

        let mut it = ansistring.iter();
        assert_eq!(Some(('a', Attr::default())), it.next());
        assert_eq!(Some(('b', Attr::default())), it.next());
        assert_eq!(None, it.next());

        assert_eq!("ab", ansistring.into_inner())
    }
}

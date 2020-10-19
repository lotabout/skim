// Parse ANSI attr code
use std::default::Default;
use std::mem;

use beef::lean::Cow;
use std::cmp::max;
use tuikit::prelude::*;
use vte::Perform;

/// An ANSI Parser, will parse one line at a time.
///
/// It will cache the latest attribute used, that means if an attribute affect multiple
/// lines, the parser will recognize it.
pub struct ANSIParser {
    partial_str: String,
    last_attr: Attr,

    stripped: String,
    fragments: Vec<(Attr, (u32, u32))>,
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
            // \b to delete character back
            0x08 => {
                self.partial_str.pop();
            }
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

        // \[[m => means reset
        let mut attr = if params.is_empty() {
            Attr::default()
        } else {
            self.last_attr
        };

        let mut iter = params.iter();
        while let Some(&code) = iter.next() {
            match code {
                0 => attr = Attr::default(),
                1 => attr.effect |= Effect::BOLD,
                2 => attr.effect |= !Effect::BOLD,
                4 => attr.effect |= Effect::UNDERLINE,
                5 => attr.effect |= Effect::BLINK,
                7 => attr.effect |= Effect::REVERSE,
                num if num >= 30 && num <= 37 => {
                    attr.fg = Color::AnsiValue((num - 30) as u8);
                }
                38 => match iter.next() {
                    Some(2) => {
                        // ESC[ 38;2;<r>;<g>;<b> m Select RGB foreground color
                        let or = iter.next();
                        let og = iter.next();
                        let ob = iter.next();
                        if ob.is_none() {
                            trace!("ignore CSI {:?} m", params);
                            continue;
                        }

                        let r = *or.unwrap() as u8;
                        let g = *og.unwrap() as u8;
                        let b = *ob.unwrap() as u8;

                        attr.fg = Color::Rgb(r, g, b);
                    }
                    Some(5) => {
                        // ESC[ 38;5;<n> m Select foreground color
                        let color = iter.next();
                        if color.is_none() {
                            trace!("ignore CSI {:?} m", params);
                            continue;
                        }
                        attr.fg = Color::AnsiValue(*color.unwrap() as u8);
                    }
                    _ => {
                        trace!("error on parsing CSI {:?} m", params);
                    }
                },
                39 => attr.fg = Color::Default,
                num if num >= 40 && num <= 47 => {
                    attr.bg = Color::AnsiValue((num - 40) as u8);
                }
                48 => match iter.next() {
                    Some(2) => {
                        // ESC[ 48;2;<r>;<g>;<b> m Select RGB background color
                        let or = iter.next();
                        let og = iter.next();
                        let ob = iter.next();
                        if ob.is_none() {
                            trace!("ignore CSI {:?} m", params);
                            continue;
                        }

                        let r = *or.unwrap() as u8;
                        let g = *og.unwrap() as u8;
                        let b = *ob.unwrap() as u8;

                        attr.bg = Color::Rgb(r, g, b);
                    }
                    Some(5) => {
                        // ESC[ 48;5;<n> m Select background color
                        let color = iter.next();
                        if color.is_none() {
                            trace!("ignore CSI {:?} m", params);
                            continue;
                        }
                        attr.bg = Color::AnsiValue(*color.unwrap() as u8);
                    }
                    _ => {
                        trace!("ignore CSI {:?} m", params);
                    }
                },
                49 => attr.bg = Color::Default,
                _ => {
                    trace!("ignore CSI {:?} m", params);
                }
            }
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
        self.fragments.push((
            self.last_attr,
            (self.stripped.len() as u32, (self.stripped.len() + string.len()) as u32),
        ));
        self.stripped.push_str(&string);
    }

    // accept a new attr
    fn attr_change(&mut self, new_attr: Attr) {
        if new_attr == self.last_attr {
            return;
        }

        self.save_str();
        self.last_attr = new_attr;
    }

    pub fn parse_ansi(&mut self, text: &str) -> AnsiString<'static> {
        let mut statemachine = vte::Parser::new();

        for byte in text.as_bytes() {
            statemachine.advance(self, *byte);
        }
        self.save_str();

        let stripped = mem::replace(&mut self.stripped, String::new());
        let fragments = mem::replace(&mut self.fragments, Vec::new());
        AnsiString::new_string(stripped, fragments)
    }
}

/// A String that contains ANSI state (e.g. colors)
///
/// It is internally represented as Vec<(attr, string)>
#[derive(Clone, Debug)]
pub struct AnsiString<'a> {
    stripped: Cow<'a, str>,
    // attr: start, end
    fragments: Option<Vec<(Attr, (u32, u32))>>,
}

impl<'a> AnsiString<'a> {
    pub fn new_empty() -> Self {
        Self {
            stripped: Cow::borrowed(""),
            fragments: None,
        }
    }

    fn new_raw_string(string: String) -> Self {
        Self {
            stripped: Cow::owned(string),
            fragments: None,
        }
    }

    fn new_raw_str(str_ref: &'a str) -> Self {
        Self {
            stripped: Cow::borrowed(str_ref),
            fragments: None,
        }
    }

    /// assume the fragments are ordered by (start, end) while end is exclusive
    pub fn new_str(stripped: &'a str, fragments: Vec<(Attr, (u32, u32))>) -> Self {
        let fragments_empty = fragments.is_empty() || (fragments.len() == 1 && fragments[0].0 == Attr::default());
        Self {
            stripped: Cow::borrowed(stripped),
            fragments: if fragments_empty { None } else { Some(fragments) },
        }
    }

    /// assume the fragments are ordered by (start, end) while end is exclusive
    pub fn new_string(stripped: String, fragments: Vec<(Attr, (u32, u32))>) -> Self {
        let fragments_empty = fragments.is_empty() || (fragments.len() == 1 && fragments[0].0 == Attr::default());
        Self {
            stripped: Cow::owned(stripped),
            fragments: if fragments_empty { None } else { Some(fragments) },
        }
    }

    pub fn parse(raw: &'a str) -> AnsiString<'static> {
        ANSIParser::default().parse_ansi(raw)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.stripped.is_empty()
    }

    #[inline]
    pub fn into_inner(self) -> std::borrow::Cow<'a, str> {
        std::borrow::Cow::Owned(self.stripped.into_owned())
    }

    pub fn iter(&'a self) -> Box<dyn Iterator<Item = (char, Attr)> + 'a> {
        if self.fragments.is_none() {
            return Box::new(self.stripped.chars().map(|c| (c, Attr::default())));
        }

        Box::new(AnsiStringIterator::new(
            &self.stripped,
            self.fragments.as_ref().unwrap(),
        ))
    }

    pub fn has_attrs(&self) -> bool {
        self.fragments.is_some()
    }

    #[inline]
    pub fn stripped(&self) -> &str {
        &self.stripped
    }

    pub fn override_attrs(&mut self, attrs: Vec<(Attr, (u32, u32))>) {
        if attrs.is_empty() {
            // pass
        } else if self.fragments.is_none() {
            self.fragments = Some(attrs);
        } else {
            let current_fragments = self.fragments.take().expect("unreachable");
            let new_fragments = merge_fragments(&current_fragments, &attrs);
            self.fragments.replace(new_fragments);
        }
    }
}

impl<'a> From<&'a str> for AnsiString<'a> {
    fn from(s: &'a str) -> AnsiString<'a> {
        AnsiString::new_raw_str(s)
    }
}

impl From<String> for AnsiString<'static> {
    fn from(s: String) -> Self {
        AnsiString::new_raw_string(s)
    }
}

// (text, indices, highlight attribute) -> AnsiString
impl<'a> From<(&'a str, &'a [usize], Attr)> for AnsiString<'a> {
    fn from((text, indices, attr): (&'a str, &'a [usize], Attr)) -> Self {
        let fragments = indices
            .iter()
            .map(|&idx| (attr, (idx as u32, 1 + idx as u32)))
            .collect();
        AnsiString::new_str(text, fragments)
    }
}

/// An iterator over all the (char, attr) characters.
pub struct AnsiStringIterator<'a> {
    fragments: &'a [(Attr, (u32, u32))],
    fragment_idx: usize,
    chars_iter: std::str::CharIndices<'a>,
}

impl<'a> AnsiStringIterator<'a> {
    pub fn new(stripped: &'a str, fragments: &'a [(Attr, (u32, u32))]) -> Self {
        Self {
            fragments,
            fragment_idx: 0,
            chars_iter: stripped.char_indices(),
        }
    }
}

impl<'a> Iterator for AnsiStringIterator<'a> {
    type Item = (char, Attr);

    fn next(&mut self) -> Option<Self::Item> {
        match self.chars_iter.next() {
            Some((char_idx, char)) => {
                // update fragment_idx
                loop {
                    if self.fragment_idx >= self.fragments.len() {
                        break;
                    }

                    let (_attr, (_start, end)) = self.fragments[self.fragment_idx];
                    if char_idx < (end as usize) {
                        break;
                    } else {
                        self.fragment_idx += 1;
                    }
                }

                let (attr, (start, end)) = if self.fragment_idx >= self.fragments.len() {
                    (Attr::default(), (char_idx as u32, 1 + char_idx as u32))
                } else {
                    self.fragments[self.fragment_idx]
                };

                if (start as usize) <= char_idx && char_idx < (end as usize) {
                    Some((char, attr))
                } else {
                    Some((char, Attr::default()))
                }
            }
            None => None,
        }
    }
}

fn merge_fragments(old: &[(Attr, (u32, u32))], new: &[(Attr, (u32, u32))]) -> Vec<(Attr, (u32, u32))> {
    let mut ret = vec![];
    let mut i = 0;
    let mut j = 0;
    let mut os = 0;

    while i < old.len() && j < new.len() {
        let (oa, (o_start, oe)) = old[i];
        let (na, (ns, ne)) = new[j];
        os = max(os, o_start);

        if ns <= os && ne >= oe {
            //   [--old--]   | [--old--]   |   [--old--] | [--old--]
            // [----new----] | [---new---] | [---new---] | [--new--]
            i += 1; // skip old
        } else if ns <= os {
            //           [--old--] |         [--old--] |   [--old--] |   [---old---]
            // [--new--]           | [--new--]         | [--new--]   |   [--new--]
            ret.push((na, (ns, ne)));
            os = ne;
            j += 1;
        } else if ns >= oe {
            // [--old--]         | [--old--]
            //         [--new--] |           [--new--]
            ret.push((oa, (os, oe)));
            i += 1;
        } else {
            // [---old---] | [---old---] | [--old--]
            //  [--new--]  |   [--new--] |      [--new--]
            ret.push((oa, (os, ns)));
            os = ns;
        }
    }

    if i < old.len() {
        for &(oa, (s, e)) in old[i..].iter() {
            ret.push((oa, (max(os, s), e)))
        }
    }
    if j < new.len() {
        ret.extend_from_slice(&new[j..]);
    }

    ret
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
        assert_eq!(ansistring.stripped(), "hi");
    }

    #[test]
    fn test_highlight_indices() {
        let text = "abc";
        let indices: Vec<usize> = vec![1];
        let attr = Attr {
            fg: Color::Rgb(70, 130, 180),
            bg: Color::Rgb(5, 10, 15),
            ..Attr::default()
        };

        let ansistring = AnsiString::from((text, &indices as &[usize], attr));
        let mut it = ansistring.iter();

        assert_eq!(Some(('a', Attr::default())), it.next());
        assert_eq!(Some(('b', attr)), it.next());
        assert_eq!(Some(('c', Attr::default())), it.next());
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

        assert_eq!(ansistring.stripped(), "ab");
    }

    #[test]
    fn test_multiple_attributes() {
        let input = "\x1B[1;31mhi";
        let ansistring = ANSIParser::default().parse_ansi(input);
        let mut it = ansistring.iter();
        let attr = Attr {
            fg: Color::AnsiValue(1),
            effect: Effect::BOLD,
            ..Attr::default()
        };

        assert_eq!(Some(('h', attr)), it.next());
        assert_eq!(Some(('i', attr)), it.next());
        assert_eq!(None, it.next());
        assert_eq!(ansistring.stripped(), "hi");
    }

    #[test]
    fn test_reset() {
        let input = "\x1B[35mA\x1B[mB";
        let ansistring = ANSIParser::default().parse_ansi(input);
        assert_eq!(ansistring.fragments.as_ref().map(|x| x.len()).unwrap(), 2);
        assert_eq!(ansistring.stripped(), "AB");
    }

    #[test]
    fn test_merge_fragments() {
        let ao = Attr::default();
        let an = Attr::default().bg(Color::BLUE);

        assert_eq!(
            merge_fragments(&[(ao, (0, 1)), (ao, (1, 2))], &[]),
            vec![(ao, (0, 1)), (ao, (1, 2))]
        );

        assert_eq!(
            merge_fragments(&[], &[(an, (0, 1)), (an, (1, 2))]),
            vec![(an, (0, 1)), (an, (1, 2))]
        );

        assert_eq!(
            merge_fragments(&[(ao, (1, 3)), (ao, (5, 6)), (ao, (9, 10))], &[(an, (0, 1))]),
            vec![(an, (0, 1)), (ao, (1, 3)), (ao, (5, 6)), (ao, (9, 10))]
        );

        assert_eq!(
            merge_fragments(&[(ao, (1, 3)), (ao, (5, 7)), (ao, (9, 11))], &[(an, (0, 2))]),
            vec![(an, (0, 2)), (ao, (2, 3)), (ao, (5, 7)), (ao, (9, 11))]
        );

        assert_eq!(
            merge_fragments(&[(ao, (1, 3)), (ao, (5, 7)), (ao, (9, 11))], &[(an, (0, 3))]),
            vec![(an, (0, 3)), (ao, (5, 7)), (ao, (9, 11))]
        );

        assert_eq!(
            merge_fragments(
                &[(ao, (1, 3)), (ao, (5, 7)), (ao, (9, 11))],
                &[(an, (0, 6)), (an, (6, 7))]
            ),
            vec![(an, (0, 6)), (an, (6, 7)), (ao, (9, 11))]
        );

        assert_eq!(
            merge_fragments(&[(ao, (1, 3)), (ao, (5, 7)), (ao, (9, 11))], &[(an, (1, 2))]),
            vec![(an, (1, 2)), (ao, (2, 3)), (ao, (5, 7)), (ao, (9, 11))]
        );

        assert_eq!(
            merge_fragments(&[(ao, (1, 3)), (ao, (5, 7)), (ao, (9, 11))], &[(an, (1, 3))]),
            vec![(an, (1, 3)), (ao, (5, 7)), (ao, (9, 11))]
        );

        assert_eq!(
            merge_fragments(&[(ao, (1, 3)), (ao, (5, 7)), (ao, (9, 11))], &[(an, (1, 4))]),
            vec![(an, (1, 4)), (ao, (5, 7)), (ao, (9, 11))]
        );

        assert_eq!(
            merge_fragments(&[(ao, (1, 3)), (ao, (5, 7)), (ao, (9, 11))], &[(an, (2, 3))]),
            vec![(ao, (1, 2)), (an, (2, 3)), (ao, (5, 7)), (ao, (9, 11))]
        );

        assert_eq!(
            merge_fragments(&[(ao, (1, 3)), (ao, (5, 7)), (ao, (9, 11))], &[(an, (2, 4))]),
            vec![(ao, (1, 2)), (an, (2, 4)), (ao, (5, 7)), (ao, (9, 11))]
        );

        assert_eq!(
            merge_fragments(&[(ao, (1, 3)), (ao, (5, 7)), (ao, (9, 11))], &[(an, (2, 6))]),
            vec![(ao, (1, 2)), (an, (2, 6)), (ao, (6, 7)), (ao, (9, 11))]
        );
    }
}

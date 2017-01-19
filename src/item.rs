// An item is line of text that read from `find` command or stdin together with
// the internal states, such as selected or not

use std::cmp::Ordering;
use ncurses::*;
use ansi::parse_ansi;
use regex::Regex;
use reader::FieldRange;
use std::borrow::Cow;
use std::ascii::AsciiExt;
use std::sync::Arc;

macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

// An item will store everything that one line input will need to be operated and displayed.
//
// What's special about an item?
// The simplest version of an item is a line of string, but things are getting more complex:
// - The conversion of lower/upper case is slow in rust, because it involds unicode.
// - We may need to interpret the ANSI codes in the text.
// - The text can be transformed and limited while searching.

// About the ANSI, we made assumption that it is linewise, that means no ANSI codes will affect
// more than one line.

#[derive(Debug)]
pub struct Item {
    // (num of run, number of index)
    index: (usize, usize),

    // The text that will be ouptut when user press `enter`
    output_text: String,

    // The text that will shown into the screen. Can be transformed.
    text: String,

    // cache of the lower case version of text. To improve speed
    text_lower_chars: Vec<char>,

    // the ansi state (color) of the text
    ansi_states: Vec<(usize, attr_t)>,
    matching_ranges: Vec<(usize, usize)>,

    // For the transformed ANSI case, the output will need another transform.
    using_transform_fields: bool,
    ansi_enabled: bool,
}

impl<'a> Item {
    pub fn new(orig_text: String,
               ansi_enabled: bool,
               trans_fields: &[FieldRange],
               matching_fields: &[FieldRange],
               delimiter: &Regex,
               index: (usize, usize)) -> Self {
        let using_transform_fields = trans_fields.len() > 0;

        //        transformed | ANSI             | output
        //------------------------------------------------------
        //                    +- T -> trans+ANSI | ANSI
        //                    |                  |
        //      +- T -> trans +- F -> trans      | orig
        // orig |                                |
        //      +- F -> orig  +- T -> ANSI     ==| ANSI
        //                    |                  |
        //                    +- F -> orig       | orig

        let (text, states_text) = if using_transform_fields && ansi_enabled {
            // ansi and transform
            parse_ansi(&parse_transform_fields(delimiter, &orig_text, trans_fields))
        } else if using_transform_fields {
            // transformed, not ansi
            (parse_transform_fields(delimiter, &orig_text, trans_fields), Vec::new())
        } else if ansi_enabled {
            // not transformed, ansi
            parse_ansi(&orig_text)
        } else {
            // normal case
            ("".to_string(), Vec::new())
        };

        let mut ret = Item {
            index: index,
            output_text: orig_text,
            text: text,
            text_lower_chars: Vec::new(),
            ansi_states: states_text,
            using_transform_fields: trans_fields.len() > 0,
            matching_ranges: Vec::new(),
            ansi_enabled: ansi_enabled,
        };

        let lower_chars: Vec<char> = ret.get_text().to_ascii_lowercase().chars().collect();
        let matching_ranges = if matching_fields.len() > 0 {
            parse_matching_fields(delimiter, ret.get_text(), matching_fields)
        } else {
            vec![(0, lower_chars.len())]
        };

        ret.text_lower_chars = lower_chars;
        ret.matching_ranges = matching_ranges;
        ret
    }

    pub fn get_text(&self) -> &str {
        if !self.using_transform_fields && !self.ansi_enabled {
            &self.output_text
        } else {
            &self.text
        }
    }

    pub fn get_output_text(&'a self) -> Cow<'a, str> {
        if self.using_transform_fields && self.ansi_enabled {
            let (text, _) = parse_ansi(&self.output_text);
            Cow::Owned(text)
        } else if !self.using_transform_fields && self.ansi_enabled {
            Cow::Borrowed(&self.text)
        } else {
            Cow::Borrowed(&self.output_text)
        }
    }

    pub fn get_lower_chars(&self) -> &[char] {
        &self.text_lower_chars
    }

    pub fn get_ansi_states(&self) -> &Vec<(usize, attr_t)> {
        &self.ansi_states
    }

    pub fn get_index(&self) -> usize {
        self.index.1
    }

    pub fn get_full_index(&self) -> (usize, usize) {
        self.index
    }

    pub fn get_matching_ranges(&self) -> &[(usize, usize)] {
        &self.matching_ranges
    }
}

impl Clone for Item {
    fn clone(&self) -> Item {
        Item {
            index: self.index,
            output_text: self.output_text.clone(),
            text: self.text.clone(),
            text_lower_chars: self.text_lower_chars.clone(),
            ansi_states: self.ansi_states.clone(),
            using_transform_fields: self.using_transform_fields,
            matching_ranges: self.matching_ranges.clone(),
            ansi_enabled: self.ansi_enabled,
        }
    }
}

fn parse_transform_fields(delimiter: &Regex, text: &str, fields: &[FieldRange]) -> String {
    let mut ranges =  delimiter.find_iter(text)
        .map(|m| (m.start(), m.end()))
        .collect::<Vec<(usize, usize)>>();
    let &(_, end) = ranges.last().unwrap_or(&(0, 0));
    ranges.push((end, text.len()));

    let mut ret = String::new();
    for field in fields {
        if let Some((start, stop)) = parse_field_range(field, ranges.len()) {
            let &(begin, _) = ranges.get(start).unwrap();
            let &(end, _) = ranges.get(stop).unwrap_or(&(text.len(), 0));
            ret.push_str(&text[begin..end]);
        }
    }
    ret
}

fn parse_matching_fields(delimiter: &Regex, text: &str, fields: &[FieldRange]) -> Vec<(usize, usize)> {
    let mut ranges =  delimiter.find_iter(text)
        .map(|m| (m.start(), m.end()))
        .collect::<Vec<(usize, usize)>>();
    let &(_, end) = ranges.last().unwrap_or(&(0, 0));
    ranges.push((end, text.len()));

    let mut ret = Vec::new();
    for field in fields {
        if let Some((start, stop)) = parse_field_range(field, ranges.len()) {
            let &(begin, _) = ranges.get(start).unwrap();
            let &(end, _) = ranges.get(stop).unwrap_or(&(text.len(), 0));
            let first = (&text[..begin]).chars().count();
            let last = first + (&text[begin..end]).chars().count();
            ret.push((first, last));
        }
    }
    ret
}

fn parse_field_range(range: &FieldRange, length: usize) -> Option<(usize, usize)> {
    let length = length as i64;
    match *range {
        FieldRange::Single(index) => {
            let index = if index >= 0 {index} else {length + index};
            if index < 0 || index >= length {
                None
            } else {
                Some((index as usize, (index + 1) as usize))
            }
        }
        FieldRange::LeftInf(right) => {
            let right = if right >= 0 {right} else {length + right};
            if right <= 0 {
                None
            } else {
                Some((0, if right > length {length as usize} else {right as usize}))
            }
        }
        FieldRange::RightInf(left) => {
            let left = if left >= 0 {left} else {length as i64 + left};
            if left >= length {
                None
            } else {
                Some((if left < 0 {0} else {left} as usize, length as usize))
            }
        }
        FieldRange::Both(left, right) => {
            let left = if left >= 0 {left} else {length + left};
            let right = if right >= 0 {right} else {length + right};
            if left >= right || left >= length || right < 0 {
                None
            } else {
                Some((if left < 0 {0} else {left as usize},
                      if right > length {length as usize} else {right as usize}))
            }
        }
    }
}

// A bunch of items
pub type ItemGroup = Vec<Arc<Item>>;
pub type MatchedItemGroup = Vec<MatchedItem>;

pub type Rank = [i64; 4]; // score, index, start, end


#[derive(PartialEq, Eq, Clone, Debug)]
#[allow(dead_code)]
pub enum MatchedRange {
    Range(usize, usize),
    Chars(Vec<usize>),
}

#[derive(Clone, Debug)]
pub struct MatchedItem {
    pub item: Arc<Item>,
    pub rank: Rank,
    pub matched_range: Option<MatchedRange>,  // range of chars that metched the pattern
}

impl MatchedItem {
    pub fn builder(item: Arc<Item>) -> Self {
        MatchedItem {
            item: item,
            rank: [0, 0, 0, 0],
            matched_range: None,
        }
    }

    pub fn matched_range(mut self, range: MatchedRange) -> Self{
        self.matched_range = Some(range);
        self
    }

    pub fn rank(mut self, rank: Rank) -> Self {
        self.rank = rank;
        self
    }

    pub fn build(self) -> Self {
        self
    }
}

impl Ord for MatchedItem {
    fn cmp(&self, other: &MatchedItem) -> Ordering {
        self.rank.cmp(&other.rank)
    }
}

// `PartialOrd` needs to be implemented as well.
impl PartialOrd for MatchedItem {
    fn partial_cmp(&self, other: &MatchedItem) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for MatchedItem {
    fn eq(&self, other: &MatchedItem) -> bool {
        self.rank == other.rank
    }
}

impl Eq for MatchedItem {}

#[cfg(test)]
mod test {
    use reader::FieldRange::*;
    use regex::Regex;

    #[test]
    fn test_parse_field_range() {
        assert_eq!(super::parse_field_range(&Single(0), 10), Some((0,1)));
        assert_eq!(super::parse_field_range(&Single(9), 10), Some((9,10)));
        assert_eq!(super::parse_field_range(&Single(10), 10), None);
        assert_eq!(super::parse_field_range(&Single(-1), 10), Some((9,10)));
        assert_eq!(super::parse_field_range(&Single(-10), 10), Some((0,1)));
        assert_eq!(super::parse_field_range(&Single(-11), 10), None);

        assert_eq!(super::parse_field_range(&LeftInf(0), 10), None);
        assert_eq!(super::parse_field_range(&LeftInf(1), 10), Some((0,1)));
        assert_eq!(super::parse_field_range(&LeftInf(8), 10), Some((0,8)));
        assert_eq!(super::parse_field_range(&LeftInf(10), 10), Some((0,10)));
        assert_eq!(super::parse_field_range(&LeftInf(11), 10), Some((0,10)));
        assert_eq!(super::parse_field_range(&LeftInf(-1), 10), Some((0,9)));
        assert_eq!(super::parse_field_range(&LeftInf(-8), 10), Some((0,2)));
        assert_eq!(super::parse_field_range(&LeftInf(-9), 10), Some((0,1)));
        assert_eq!(super::parse_field_range(&LeftInf(-10), 10), None);
        assert_eq!(super::parse_field_range(&LeftInf(-11), 10), None);

        assert_eq!(super::parse_field_range(&RightInf(0), 10), Some((0,10)));
        assert_eq!(super::parse_field_range(&RightInf(1), 10), Some((1,10)));
        assert_eq!(super::parse_field_range(&RightInf(8), 10), Some((8,10)));
        assert_eq!(super::parse_field_range(&RightInf(10), 10), None);
        assert_eq!(super::parse_field_range(&RightInf(11), 10), None);
        assert_eq!(super::parse_field_range(&RightInf(-1), 10), Some((9,10)));
        assert_eq!(super::parse_field_range(&RightInf(-8), 10), Some((2,10)));
        assert_eq!(super::parse_field_range(&RightInf(-9), 10), Some((1,10)));
        assert_eq!(super::parse_field_range(&RightInf(-10), 10), Some((0, 10)));
        assert_eq!(super::parse_field_range(&RightInf(-11), 10), Some((0, 10)));

        assert_eq!(super::parse_field_range(&Both(0,0), 10), None);
        assert_eq!(super::parse_field_range(&Both(0,1), 10), Some((0,1)));
        assert_eq!(super::parse_field_range(&Both(0,10), 10), Some((0,10)));
        assert_eq!(super::parse_field_range(&Both(0,11), 10), Some((0, 10)));
        assert_eq!(super::parse_field_range(&Both(1,-1), 10), Some((1, 9)));
        assert_eq!(super::parse_field_range(&Both(1,-9), 10), None);
        assert_eq!(super::parse_field_range(&Both(1,-10), 10), None);
        assert_eq!(super::parse_field_range(&Both(-9,-9), 10), None);
        assert_eq!(super::parse_field_range(&Both(-9,-8), 10), Some((1, 2)));
        assert_eq!(super::parse_field_range(&Both(-9, 0), 10), None);
        assert_eq!(super::parse_field_range(&Both(-9, 1), 10), None);
        assert_eq!(super::parse_field_range(&Both(-9, 2), 10), Some((1,2)));
        assert_eq!(super::parse_field_range(&Both(-1,0), 10), None);
        assert_eq!(super::parse_field_range(&Both(11,20), 10), None);
        assert_eq!(super::parse_field_range(&Both(-10,-10), 10), None);
    }

    #[test]
    fn test_parse_transform_fields() {
        // delimiter is ","
        let re = Regex::new(".*?,").unwrap();

        assert_eq!(super::parse_transform_fields(&re, &"A,B,C,D,E,F",
                                                 &vec![Single(1),
                                                       Single(3),
                                                       Single(-1),
                                                       Single(-7)]),
                   "B,D,F");

        assert_eq!(super::parse_transform_fields(&re, &"A,B,C,D,E,F",
                                                 &vec![LeftInf(3),
                                                       LeftInf(-5),
                                                       LeftInf(-6)]),
                   "A,B,C,A,");

        assert_eq!(super::parse_transform_fields(&re, &"A,B,C,D,E,F",
                                                 &vec![RightInf(4),
                                                       RightInf(-2),
                                                       RightInf(-1),
                                                       RightInf(7)]),
                   "E,FE,FF");

        assert_eq!(super::parse_transform_fields(&re, &"A,B,C,D,E,F",
                                                 &vec![Both(2,3),
                                                       Both(-9,2),
                                                       Both(5,10),
                                                       Both(-9,-4)]),
                   "C,A,B,FA,B,");
    }

    #[test]
    fn test_parse_matching_fields() {
        // delimiter is ","
        let re = Regex::new(".*?,").unwrap();

        assert_eq!(super::parse_matching_fields(&re, &"中,华,人,民,E,F",
                                                &vec![Single(1),
                                                      Single(3),
                                                      Single(-1),
                                                      Single(-7)]),
                   vec![(2,4), (6,8), (10,11)]);

        assert_eq!(super::parse_matching_fields(&re, &"中,华,人,民,E,F",
                                                &vec![LeftInf(3),
                                                      LeftInf(-5),
                                                      LeftInf(-6)]),
                   vec![(0, 6), (0, 2)]);

        assert_eq!(super::parse_matching_fields(&re, &"中,华,人,民,E,F",
                                                &vec![RightInf(4),
                                                      RightInf(-2),
                                                      RightInf(-1),
                                                      RightInf(7)]),
                   vec![(8, 11), (8, 11), (10, 11)]);

        assert_eq!(super::parse_matching_fields(&re, &"中,华,人,民,E,F",
                                                &vec![Both(2,3),
                                                      Both(-9,2),
                                                      Both(5,10),
                                                      Both(-9,-4)]),
                   vec![(4, 6), (0, 4), (10,11), (0, 4)]);
    }
}

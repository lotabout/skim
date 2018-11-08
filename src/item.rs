// An item is line of text that read from `find` command or stdin together with
// the internal states, such as selected or not

use ansi::{ANSIParser, AnsiString};
use curses::attr_t;
use field::*;
use regex::Regex;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::default::Default;
use std::sync::Arc;

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
    orig_text: String,

    // The text that will shown into the screen. Can be transformed.
    text: AnsiString,

    // cache of the lower case version of text. To improve speed
    chars: Vec<char>,

    matching_ranges: Vec<(usize, usize)>,

    // For the transformed ANSI case, the output will need another transform.
    using_transform_fields: bool,
    ansi_enabled: bool,
}

impl<'a> Item {
    pub fn new(
        orig_text: String,
        ansi_enabled: bool,
        trans_fields: &[FieldRange],
        matching_fields: &[FieldRange],
        delimiter: &Regex,
        index: (usize, usize),
    ) -> Self {
        let using_transform_fields = !trans_fields.is_empty();

        //        transformed | ANSI             | output
        //------------------------------------------------------
        //                    +- T -> trans+ANSI | ANSI
        //                    |                  |
        //      +- T -> trans +- F -> trans      | orig
        // orig |                                |
        //      +- F -> orig  +- T -> ANSI     ==| ANSI
        //                    |                  |
        //                    +- F -> orig       | orig

        let mut ansi_parser: ANSIParser = Default::default();

        let text = if using_transform_fields && ansi_enabled {
            // ansi and transform
            ansi_parser.parse_ansi(&parse_transform_fields(delimiter, &orig_text, trans_fields))
        } else if using_transform_fields {
            // transformed, not ansi
            AnsiString{stripped: parse_transform_fields(delimiter, &orig_text, trans_fields), ansi_states: Vec::new()}
        } else if ansi_enabled {
            // not transformed, ansi
            ansi_parser.parse_ansi(&orig_text)
        } else {
            // normal case
            AnsiString::new_empty()
        };

        let mut ret = Item {
            index,
            orig_text,
            text,
            chars: Vec::new(),
            using_transform_fields: !trans_fields.is_empty(),
            matching_ranges: Vec::new(),
            ansi_enabled,
        };

        let chars: Vec<char> = if ret.get_text().as_bytes().is_ascii() {
            ret.get_text().as_bytes().iter().map(|&s| s as char).collect()
        } else {
            ret.get_text().chars().collect()
        };

        let matching_ranges = if !matching_fields.is_empty() {
            parse_matching_fields(delimiter, ret.get_text(), matching_fields)
        } else {
            vec![(0, chars.len())]
        };

        ret.chars = chars;
        ret.matching_ranges = matching_ranges;
        ret
    }

    pub fn get_text(&self) -> &str {
        if !self.using_transform_fields && !self.ansi_enabled {
            &self.orig_text
        } else {
            &self.text.stripped
        }
    }

    pub fn get_orig_text(&'a self) -> Cow<'a, str> {
        Cow::Borrowed(&self.orig_text)
    }

    pub fn get_text_struct(&self) -> Option<&AnsiString> {
        if !self.using_transform_fields && !self.ansi_enabled {
            None
        } else {
            Some(&self.text)
        }
    }

    pub fn get_output_text(&'a self) -> Cow<'a, str> {
        if self.using_transform_fields && self.ansi_enabled {
            let mut ansi_parser: ANSIParser = Default::default();
            let text = ansi_parser.parse_ansi(&self.orig_text);
            Cow::Owned(text.into_inner())
        } else if !self.using_transform_fields && self.ansi_enabled {
            Cow::Borrowed(&self.text.stripped)
        } else {
            Cow::Borrowed(&self.orig_text)
        }
    }

    pub fn get_chars(&self) -> &[char] {
        &self.chars
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
            orig_text: self.orig_text.clone(),
            text: self.text.clone(),
            chars: self.chars.clone(),
            using_transform_fields: self.using_transform_fields,
            matching_ranges: self.matching_ranges.clone(),
            ansi_enabled: self.ansi_enabled,
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
    pub matched_range: Option<MatchedRange>, // range of chars that metched the pattern
}

impl MatchedItem {
    pub fn builder(item: Arc<Item>) -> Self {
        MatchedItem {
            item: item,
            rank: [0, 0, 0, 0],
            matched_range: None,
        }
    }

    pub fn matched_range(mut self, range: MatchedRange) -> Self {
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

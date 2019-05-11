///! An item is line of text that read from `find` command or stdin together with
///! the internal states, such as selected or not
use crate::ansi::{ANSIParser, AnsiString};
use crate::field::*;
use crate::spinlock::{SpinLock, SpinLockGuard};
use regex::Regex;
use std::borrow::Cow;
use std::cmp::min;
use std::default::Default;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// An item will store everything that one line input will need to be operated and displayed.
///
/// What's special about an item?
/// The simplest version of an item is a line of string, but things are getting more complex:
/// - The conversion of lower/upper case is slow in rust, because it involds unicode.
/// - We may need to interpret the ANSI codes in the text.
/// - The text can be transformed and limited while searching.
///
/// About the ANSI, we made assumption that it is linewise, that means no ANSI codes will affect
/// more than one line.
#[derive(Debug)]
pub struct Item {
    // (num of run, number of index)
    index: (usize, usize),

    // The text that will be ouptut when user press `enter`
    orig_text: String,

    // The text that will shown into the screen. Can be transformed.
    text: AnsiString,

    matching_ranges: Vec<(usize, usize)>,

    // For the transformed ANSI case, the output will need another transform.
    using_transform_fields: bool,
    ansi_enabled: bool,
}

impl<'a> Item {
    pub fn new(
        orig_text: Cow<str>,
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
            AnsiString::new_string(parse_transform_fields(delimiter, &orig_text, trans_fields))
        } else if ansi_enabled {
            // not transformed, ansi
            ansi_parser.parse_ansi(&orig_text)
        } else {
            // normal case
            AnsiString::new_empty()
        };

        let mut ret = Item {
            index,
            orig_text: orig_text.into_owned(),
            text,
            using_transform_fields: !trans_fields.is_empty(),
            matching_ranges: Vec::new(),
            ansi_enabled,
        };

        let matching_ranges = if !matching_fields.is_empty() {
            parse_matching_fields(delimiter, ret.get_text(), matching_fields)
        } else {
            vec![(0, ret.get_text().len())]
        };

        ret.matching_ranges = matching_ranges;
        ret
    }

    pub fn get_text(&self) -> &str {
        if !self.using_transform_fields && !self.ansi_enabled {
            &self.orig_text
        } else {
            &self.text.get_stripped()
        }
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
            Cow::Borrowed(self.text.get_stripped())
        } else {
            Cow::Borrowed(&self.orig_text)
        }
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
            using_transform_fields: self.using_transform_fields,
            matching_ranges: self.matching_ranges.clone(),
            ansi_enabled: self.ansi_enabled,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct Rank {
    pub score: i64,
    pub index: i64,
    pub begin: i64,
    pub end: i64,
}

#[derive(PartialEq, Eq, Clone, Debug)]
#[allow(dead_code)]
pub enum MatchedRange {
    ByteRange(usize, usize), // range of bytes
    Chars(Vec<usize>),       // individual characters matched
}

#[derive(Clone, Debug)]
pub struct MatchedItem {
    pub item: Arc<Item>,
    pub rank: Rank,
    pub matched_range: Option<MatchedRange>, // range of chars that matched the pattern
}

impl MatchedItem {
    pub fn builder(item: Arc<Item>) -> Self {
        MatchedItem {
            item,
            rank: Rank::default(),
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

    pub fn to_chars_range(&self) -> Option<Vec<usize>> {
      self.matched_range.as_ref().map(|r| match r {
        MatchedRange::ByteRange(start, end) => {
          self.item.orig_text[*start..*end]
            .chars()
            .map(|x| x as usize)
            .collect()
        },
        MatchedRange::Chars(vec) => vec.clone(),
      })
    }
}

const ITEM_POOL_CAPACITY: usize = 1024;

pub struct ItemPool {
    length: AtomicUsize,
    pool: SpinLock<Vec<Arc<Item>>>,
    /// number of items that was `take`n
    taken: AtomicUsize,

    /// reverse first N lines as header
    reserved_items: SpinLock<Vec<Arc<Item>>>,
    lines_to_reserve: usize,
}

impl ItemPool {
    pub fn new() -> Self {
        Self {
            length: AtomicUsize::new(0),
            pool: SpinLock::new(Vec::with_capacity(ITEM_POOL_CAPACITY)),
            taken: AtomicUsize::new(0),
            reserved_items: SpinLock::new(Vec::new()),
            lines_to_reserve: 0,
        }
    }

    pub fn lines_to_reserve(mut self, lines_to_reserve: usize) -> Self {
        self.lines_to_reserve = lines_to_reserve;
        self
    }

    pub fn len(&self) -> usize {
        self.length.load(Ordering::SeqCst)
    }

    pub fn num_not_taken(&self) -> usize {
        self.length.load(Ordering::SeqCst) - self.taken.load(Ordering::SeqCst)
    }

    pub fn clear(&self) {
        let mut items = self.pool.lock();
        items.clear();
        let mut header_items = self.reserved_items.lock();
        header_items.clear();
        self.taken.store(0, Ordering::SeqCst);
        self.length.store(0, Ordering::SeqCst);
    }

    pub fn reset(&self) {
        // lock to ensure consistency
        let _items = self.pool.lock();
        self.taken.store(0, Ordering::SeqCst);
    }

    pub fn append(&self, mut items: Vec<Arc<Item>>) {
        let mut pool = self.pool.lock();
        let mut header_items = self.reserved_items.lock();

        let to_reserve = self.lines_to_reserve - header_items.len();
        if to_reserve > 0 {
            let to_reserve = min(to_reserve, items.len());
            header_items.extend_from_slice(&items[..to_reserve]);
            pool.extend_from_slice(&items[to_reserve..]);
        } else {
            pool.append(&mut items);
        }
        self.length.store(pool.len(), Ordering::SeqCst);
    }

    pub fn take(&self) -> ItemPoolGuard<Arc<Item>> {
        let guard = self.pool.lock();
        let taken = self.taken.swap(guard.len(), Ordering::SeqCst);
        ItemPoolGuard { guard, start: taken }
    }

    pub fn reserved(&self) -> ItemPoolGuard<Arc<Item>> {
        let guard = self.reserved_items.lock();
        ItemPoolGuard { guard, start: 0 }
    }
}

pub struct ItemPoolGuard<'a, T: Sized + 'a> {
    guard: SpinLockGuard<'a, Vec<T>>,
    start: usize,
}

impl<'mutex, T: Sized> Deref for ItemPoolGuard<'mutex, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.guard[self.start..]
    }
}

//------------------------------------------------------------------------------
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RankCriteria {
    Score,
    Index,
    Begin,
    End,
    NegScore,
    NegIndex,
    NegBegin,
    NegEnd,
}

pub fn parse_criteria(text: &str) -> Option<RankCriteria> {
    match text.to_lowercase().as_ref() {
        "score" => Some(RankCriteria::Score),
        "index" => Some(RankCriteria::Index),
        "begin" => Some(RankCriteria::Begin),
        "end" => Some(RankCriteria::End),
        "-score" => Some(RankCriteria::NegScore),
        "-index" => Some(RankCriteria::NegIndex),
        "-begin" => Some(RankCriteria::NegBegin),
        "-end" => Some(RankCriteria::NegEnd),
        _ => None,
    }
}

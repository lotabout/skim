// An item is line of text that read from `find` command or stdin together with
// the internal states, such as selected or not

use std::cmp::Ordering;
use ncurses::*;
use ansi::parse_ansi;
use regex::Regex;
use reader::FieldRange;

pub struct Item {
    orig_text: String,
    pub text: String,
    text_lower_chars: Vec<char>, // lower case version of text.
    ansi_states: Vec<(usize, attr_t)>,
    using_transform_fields: bool,
    matching_ranges: Vec<(usize, usize)>,
}

impl Item {
    pub fn new(text: String,
               use_ansi: bool,
               trans_fields: &[FieldRange],
               matching_fields: &[FieldRange],
               delimiter: &Regex) -> Self {

        let (orig_text, states) = if use_ansi {
             parse_ansi(&text)
        } else {
            (text, Vec::new())
        };

        let text = if trans_fields.len() > 0 {
            parse_transform_fields(delimiter, &orig_text, trans_fields)
        } else {
            String::new()
        };

        let mut ret = Item {
            orig_text: orig_text,
            text: text,
            text_lower_chars: Vec::new(),
            ansi_states: states,
            using_transform_fields: trans_fields.len() > 0,
            matching_ranges: Vec::new(),
        };

        let lower_chars = ret.get_text().to_lowercase().chars().collect();
        let matching_ranges = if matching_fields.len() > 0 {
            parse_matching_fields(delimiter, &ret.get_text(), matching_fields)
        } else {
            Vec::new()
        };
        ret.text_lower_chars = lower_chars;
        ret.matching_ranges = matching_ranges;
        ret
    }

    pub fn get_text(&self) -> &str {
        if self.using_transform_fields {
            &self.text
        } else {
            &self.orig_text
        }
    }

    pub fn get_orig_text(&self) -> &str {
        &self.orig_text
    }

    pub fn get_lower_chars(&self) -> &[char] {
        &self.text_lower_chars
    }

    pub fn get_ansi_states(&self) -> &Vec<(usize, attr_t)> {
        &self.ansi_states
    }

    pub fn in_matching_range(&self, begin: usize, end: usize) -> bool {
        if self.matching_ranges.len() <= 0 {
            return true;
        }

        for &(start, stop) in self.matching_ranges.iter() {
            if begin >= start && end <= stop {
                return true;
            }
        }
        false
    }
}

fn parse_transform_fields(delimiter: &Regex, text: &str, fields: &[FieldRange]) -> String {
    let mut ranges =  delimiter.find_iter(text).collect::<Vec<(usize, usize)>>();
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
    let mut ranges =  delimiter.find_iter(text).collect::<Vec<(usize, usize)>>();
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



pub type Rank = [i64; 4]; // score, index, start, end


#[derive(PartialEq, Eq, Clone)]
pub enum MatchedRange {
    Range(usize, usize),
    Chars(Vec<usize>),
}

#[derive(Eq, Clone)]
pub struct MatchedItem {
    pub index: usize,                       // index of current item in items
    pub rank: Rank,
    pub matched_range: Option<MatchedRange>,  // range of chars that metched the pattern
}

impl MatchedItem {
    pub fn new(index: usize) -> Self {
        MatchedItem {
            index: index,
            rank: [0, 0, 0, 0],
            matched_range: None,
        }
    }

    pub fn set_matched_range(&mut self, range: MatchedRange) {
        self.matched_range = Some(range);
    }

    pub fn set_rank(&mut self, rank: Rank) {
        self.rank = rank;
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

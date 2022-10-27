use regex::Regex;
use std::cmp::{max, min};

lazy_static! {
    static ref FIELD_RANGE: Regex = Regex::new(r"^(?P<left>-?\d+)?(?P<sep>\.\.)?(?P<right>-?\d+)?$").unwrap();
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum FieldRange {
    Single(i32),
    LeftInf(i32),
    RightInf(i32),
    Both(i32, i32),
}

impl FieldRange {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(range: &str) -> Option<FieldRange> {
        use self::FieldRange::*;

        // "1", "1..", "..10", "1..10", etc.
        let opt_caps = FIELD_RANGE.captures(range);
        if let Some(caps) = opt_caps {
            let opt_left = caps.name("left").map(|s| s.as_str().parse().unwrap_or(1));
            let opt_right = caps.name("right").map(|s| s.as_str().parse().unwrap_or(-1));
            let opt_sep = caps.name("sep").map(|s| s.as_str().to_string());

            match (opt_left, opt_right) {
                (None, None) => Some(RightInf(0)),
                (Some(left), None) => {
                    match opt_sep {
                        None => Some(Single(left)),      // 1
                        Some(_) => Some(RightInf(left)), // 1..
                    }
                }
                (None, Some(right)) => {
                    match opt_sep {
                        None => Some(Single(right)),     // 1 (should not happen)
                        Some(_) => Some(LeftInf(right)), // ..1 (should not happen)
                    }
                }
                (Some(left), Some(right)) => Some(Both(left, right)), // 1..3
            }
        } else {
            None
        }
    }

    // Parse FieldRange to index pair (left, right)
    // e.g. 1..3 => (0, 4)
    // note that field range is inclusive while the output index will exclude right end
    pub fn to_index_pair(&self, length: usize) -> Option<(usize, usize)> {
        use self::FieldRange::*;
        match *self {
            Single(num) => {
                let num = FieldRange::translate_neg(num, length);
                if num == 0 || num > length {
                    None
                } else {
                    Some((num - 1, num))
                }
            }
            LeftInf(right) => {
                let right = FieldRange::translate_neg(right, length);
                if length == 0 || right == 0 {
                    None
                } else {
                    let right = min(right, length);
                    Some((0, right))
                }
            }
            RightInf(left) => {
                let left = FieldRange::translate_neg(left, length);
                if length == 0 || left > length {
                    None
                } else {
                    let left = max(left, 1);
                    Some((left - 1, length))
                }
            }
            Both(left, right) => {
                let left = FieldRange::translate_neg(left, length);
                let right = FieldRange::translate_neg(right, length);
                if length == 0 || right == 0 || left > right || left > length {
                    None
                } else {
                    Some((max(left, 1) - 1, min(right, length)))
                }
            }
        }
    }

    fn translate_neg(idx: i32, length: usize) -> usize {
        let len = length as i32;
        let idx = if idx < 0 { idx + len + 1 } else { idx };
        max(0, idx) as usize
    }
}

// ("|", "a|b||c") -> [(0, 2), (2, 4), (4, 5), (5, 6)]
// explain: split to ["a|", "b|", "|", "c"]
fn get_ranges_by_delimiter(delimiter: &Regex, text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut last = 0;
    for mat in delimiter.find_iter(text) {
        ranges.push((last, mat.start()));
        last = mat.end();
    }
    ranges.push((last, text.len()));
    ranges
}

// e.g. delimiter = Regex::new(",").unwrap()
// Note that this is differnt with `to_index_pair`, it uses delimiters like ".*?,"
pub fn get_string_by_field<'a>(delimiter: &Regex, text: &'a str, field: &FieldRange) -> Option<&'a str> {
    let ranges = get_ranges_by_delimiter(delimiter, text);

    if let Some((start, stop)) = field.to_index_pair(ranges.len()) {
        let &(begin, _) = &ranges[start];
        let &(_, end) = ranges.get(stop - 1).unwrap_or(&(text.len(), 0));
        Some(&text[begin..end])
    } else {
        None
    }
}

pub fn get_string_by_range<'a>(delimiter: &Regex, text: &'a str, range: &str) -> Option<&'a str> {
    FieldRange::from_str(range).and_then(|field| get_string_by_field(delimiter, text, &field))
}

// -> a vector of the matching fields (byte wise).
// Given delimiter `,`, text: "a,b,c"
// &[Single(2), LeftInf(2)] => [(2, 4), (0, 4)]
pub fn parse_matching_fields(delimiter: &Regex, text: &str, fields: &[FieldRange]) -> Vec<(usize, usize)> {
    let ranges = get_ranges_by_delimiter(delimiter, text);

    let mut ret = Vec::new();
    for field in fields {
        if let Some((start, stop)) = field.to_index_pair(ranges.len()) {
            let &(begin, _) = &ranges[start];
            let &(end, _) = ranges.get(stop).unwrap_or(&(text.len(), 0));
            ret.push((begin, end));
        }
    }
    ret
}

pub fn parse_transform_fields(delimiter: &Regex, text: &str, fields: &[FieldRange]) -> String {
    let ranges = get_ranges_by_delimiter(delimiter, text);

    let mut ret = String::new();
    for field in fields {
        if let Some((start, stop)) = field.to_index_pair(ranges.len()) {
            let &(begin, _) = &ranges[start];
            let &(end, _) = ranges.get(stop).unwrap_or(&(text.len(), 0));
            ret.push_str(&text[begin..end]);
        }
    }
    ret
}

#[cfg(test)]
mod test {
    use super::FieldRange::*;
    #[test]
    fn test_parse_range() {
        assert_eq!(FieldRange::from_str("1"), Some(Single(1)));
        assert_eq!(FieldRange::from_str("-1"), Some(Single(-1)));

        assert_eq!(FieldRange::from_str("1.."), Some(RightInf(1)));
        assert_eq!(FieldRange::from_str("-1.."), Some(RightInf(-1)));

        assert_eq!(FieldRange::from_str("..1"), Some(LeftInf(1)));
        assert_eq!(FieldRange::from_str("..-1"), Some(LeftInf(-1)));

        assert_eq!(FieldRange::from_str("1..3"), Some(Both(1, 3)));
        assert_eq!(FieldRange::from_str("-1..-3"), Some(Both(-1, -3)));

        assert_eq!(FieldRange::from_str(".."), Some(RightInf(0)));
        assert_eq!(FieldRange::from_str("a.."), None);
        assert_eq!(FieldRange::from_str("..b"), None);
        assert_eq!(FieldRange::from_str("a..b"), None);
    }

    use regex::Regex;

    #[test]
    fn test_parse_field_range() {
        assert_eq!(Single(0).to_index_pair(10), None);
        assert_eq!(Single(1).to_index_pair(10), Some((0, 1)));
        assert_eq!(Single(10).to_index_pair(10), Some((9, 10)));
        assert_eq!(Single(11).to_index_pair(10), None);
        assert_eq!(Single(-1).to_index_pair(10), Some((9, 10)));
        assert_eq!(Single(-10).to_index_pair(10), Some((0, 1)));
        assert_eq!(Single(-11).to_index_pair(10), None);

        assert_eq!(LeftInf(0).to_index_pair(10), None);
        assert_eq!(LeftInf(1).to_index_pair(10), Some((0, 1)));
        assert_eq!(LeftInf(8).to_index_pair(10), Some((0, 8)));
        assert_eq!(LeftInf(10).to_index_pair(10), Some((0, 10)));
        assert_eq!(LeftInf(11).to_index_pair(10), Some((0, 10)));
        assert_eq!(LeftInf(-1).to_index_pair(10), Some((0, 10)));
        assert_eq!(LeftInf(-8).to_index_pair(10), Some((0, 3)));
        assert_eq!(LeftInf(-9).to_index_pair(10), Some((0, 2)));
        assert_eq!(LeftInf(-10).to_index_pair(10), Some((0, 1)));
        assert_eq!(LeftInf(-11).to_index_pair(10), None);

        assert_eq!(RightInf(0).to_index_pair(10), Some((0, 10)));
        assert_eq!(RightInf(1).to_index_pair(10), Some((0, 10)));
        assert_eq!(RightInf(8).to_index_pair(10), Some((7, 10)));
        assert_eq!(RightInf(10).to_index_pair(10), Some((9, 10)));
        assert_eq!(RightInf(11).to_index_pair(10), None);
        assert_eq!(RightInf(-1).to_index_pair(10), Some((9, 10)));
        assert_eq!(RightInf(-8).to_index_pair(10), Some((2, 10)));
        assert_eq!(RightInf(-9).to_index_pair(10), Some((1, 10)));
        assert_eq!(RightInf(-10).to_index_pair(10), Some((0, 10)));
        assert_eq!(RightInf(-11).to_index_pair(10), Some((0, 10)));

        assert_eq!(Both(0, 0).to_index_pair(10), None);
        assert_eq!(Both(0, 1).to_index_pair(10), Some((0, 1)));
        assert_eq!(Both(0, 10).to_index_pair(10), Some((0, 10)));
        assert_eq!(Both(0, 11).to_index_pair(10), Some((0, 10)));
        assert_eq!(Both(1, -1).to_index_pair(10), Some((0, 10)));
        assert_eq!(Both(1, -9).to_index_pair(10), Some((0, 2)));
        assert_eq!(Both(1, -10).to_index_pair(10), Some((0, 1)));
        assert_eq!(Both(1, -11).to_index_pair(10), None);
        assert_eq!(Both(-9, -9).to_index_pair(10), Some((1, 2)));
        assert_eq!(Both(-9, -8).to_index_pair(10), Some((1, 3)));
        assert_eq!(Both(-9, 0).to_index_pair(10), None);
        assert_eq!(Both(-9, 1).to_index_pair(10), None);
        assert_eq!(Both(-9, 2).to_index_pair(10), Some((1, 2)));
        assert_eq!(Both(-1, 0).to_index_pair(10), None);
        assert_eq!(Both(11, 20).to_index_pair(10), None);
        assert_eq!(Both(-11, -11).to_index_pair(10), None);
    }

    #[test]
    fn test_parse_transform_fields() {
        // delimiter is ","
        let re = Regex::new(",").unwrap();

        assert_eq!(
            super::parse_transform_fields(&re, "A,B,C,D,E,F", &[Single(2), Single(4), Single(-1), Single(-7)]),
            "B,D,F"
        );

        assert_eq!(
            super::parse_transform_fields(&re, "A,B,C,D,E,F", &[LeftInf(3), LeftInf(-6), LeftInf(-7)]),
            "A,B,C,A,"
        );

        assert_eq!(
            super::parse_transform_fields(
                &re,
                "A,B,C,D,E,F",
                &[RightInf(5), RightInf(-2), RightInf(-1), RightInf(8)]
            ),
            "E,FE,FF"
        );

        assert_eq!(
            super::parse_transform_fields(
                &re,
                "A,B,C,D,E,F",
                &[Both(3, 3), Both(-9, 2), Both(6, 10), Both(-9, -5)]
            ),
            "C,A,B,FA,B,"
        );
    }

    #[test]
    fn test_parse_matching_fields() {
        // delimiter is ","
        let re = Regex::new(",").unwrap();

        // bytes:3  3  3 3
        //       中,华,人,民,E,F",

        assert_eq!(
            super::parse_matching_fields(&re, "中,华,人,民,E,F", &[Single(2), Single(4), Single(-1), Single(-7)]),
            vec![(4, 8), (12, 16), (18, 19)]
        );

        assert_eq!(
            super::parse_matching_fields(&re, "中,华,人,民,E,F", &[LeftInf(3), LeftInf(-6), LeftInf(-7)]),
            vec![(0, 12), (0, 4)]
        );

        assert_eq!(
            super::parse_matching_fields(
                &re,
                "中,华,人,民,E,F",
                &[RightInf(5), RightInf(-2), RightInf(-1), RightInf(7)]
            ),
            vec![(16, 19), (16, 19), (18, 19)]
        );

        assert_eq!(
            super::parse_matching_fields(
                &re,
                "中,华,人,民,E,F",
                &[Both(3, 3), Both(-8, 2), Both(6, 10), Both(-8, -5)]
            ),
            vec![(8, 12), (0, 8), (18, 19), (0, 8)]
        );
    }

    use super::*;
    #[test]
    fn test_get_string_by_field() {
        // delimiter is ","
        let re = Regex::new(",").unwrap();
        let text = "a,b,c,";
        assert_eq!(get_string_by_field(&re, text, &Single(0)), None);
        assert_eq!(get_string_by_field(&re, text, &Single(1)), Some("a"));
        assert_eq!(get_string_by_field(&re, text, &Single(2)), Some("b"));
        assert_eq!(get_string_by_field(&re, text, &Single(3)), Some("c"));
        assert_eq!(get_string_by_field(&re, text, &Single(4)), Some(""));
        assert_eq!(get_string_by_field(&re, text, &Single(5)), None);
        assert_eq!(get_string_by_field(&re, text, &Single(6)), None);
        assert_eq!(get_string_by_field(&re, text, &Single(-1)), Some(""));
        assert_eq!(get_string_by_field(&re, text, &Single(-2)), Some("c"));
        assert_eq!(get_string_by_field(&re, text, &Single(-3)), Some("b"));
        assert_eq!(get_string_by_field(&re, text, &Single(-4)), Some("a"));
        assert_eq!(get_string_by_field(&re, text, &Single(-5)), None);
        assert_eq!(get_string_by_field(&re, text, &Single(-6)), None);

        assert_eq!(get_string_by_field(&re, text, &LeftInf(0)), None);
        assert_eq!(get_string_by_field(&re, text, &LeftInf(1)), Some("a"));
        assert_eq!(get_string_by_field(&re, text, &LeftInf(2)), Some("a,b"));
        assert_eq!(get_string_by_field(&re, text, &LeftInf(3)), Some("a,b,c"));
        assert_eq!(get_string_by_field(&re, text, &LeftInf(4)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, text, &LeftInf(5)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, text, &LeftInf(-5)), None);
        assert_eq!(get_string_by_field(&re, text, &LeftInf(-4)), Some("a"));
        assert_eq!(get_string_by_field(&re, text, &LeftInf(-3)), Some("a,b"));
        assert_eq!(get_string_by_field(&re, text, &LeftInf(-2)), Some("a,b,c"));
        assert_eq!(get_string_by_field(&re, text, &LeftInf(-1)), Some("a,b,c,"));

        assert_eq!(get_string_by_field(&re, text, &RightInf(0)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, text, &RightInf(1)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, text, &RightInf(2)), Some("b,c,"));
        assert_eq!(get_string_by_field(&re, text, &RightInf(3)), Some("c,"));
        assert_eq!(get_string_by_field(&re, text, &RightInf(4)), Some(""));
        assert_eq!(get_string_by_field(&re, text, &RightInf(5)), None);
        assert_eq!(get_string_by_field(&re, text, &RightInf(-5)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, text, &RightInf(-4)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, text, &RightInf(-3)), Some("b,c,"));
        assert_eq!(get_string_by_field(&re, text, &RightInf(-2)), Some("c,"));
        assert_eq!(get_string_by_field(&re, text, &RightInf(-1)), Some(""));

        assert_eq!(get_string_by_field(&re, text, &Both(0, 0)), None);
        assert_eq!(get_string_by_field(&re, text, &Both(0, 1)), Some("a"));
        assert_eq!(get_string_by_field(&re, text, &Both(0, 2)), Some("a,b"));
        assert_eq!(get_string_by_field(&re, text, &Both(0, 3)), Some("a,b,c"));
        assert_eq!(get_string_by_field(&re, text, &Both(0, 4)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, text, &Both(0, 5)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, text, &Both(1, 1)), Some("a"));
        assert_eq!(get_string_by_field(&re, text, &Both(1, 2)), Some("a,b"));
        assert_eq!(get_string_by_field(&re, text, &Both(1, 3)), Some("a,b,c"));
        assert_eq!(get_string_by_field(&re, text, &Both(1, 4)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, text, &Both(1, 5)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, text, &Both(2, 5)), Some("b,c,"));
        assert_eq!(get_string_by_field(&re, text, &Both(3, 5)), Some("c,"));
        assert_eq!(get_string_by_field(&re, text, &Both(4, 5)), Some(""));
        assert_eq!(get_string_by_field(&re, text, &Both(5, 5)), None);
        assert_eq!(get_string_by_field(&re, text, &Both(6, 5)), None);
        assert_eq!(get_string_by_field(&re, text, &Both(2, 3)), Some("b,c"));
        assert_eq!(get_string_by_field(&re, text, &Both(3, 3)), Some("c"));
        assert_eq!(get_string_by_field(&re, text, &Both(4, 3)), None);
    }
}

use regex::Regex;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum FieldRange {
    Single(i64),
    LeftInf(i64),
    RightInf(i64),
    Both(i64, i64),
}
// e.g. delimiter = Regex::new(",").unwrap()
// Note that this is differnt with `parse_field_range`, it uses delimiters like ".*?,"
pub fn get_string_by_field<'a>(delimiter: &Regex, text: &'a str, field: &FieldRange) -> Option<&'a str> {
    let mut ranges = Vec::new();
    let mut last = 0;
    for mat in delimiter.find_iter(text) {
        ranges.push((last, mat.start()));
        last = mat.end();
    }
    ranges.push((last, text.len()));

    if let Some((start, stop)) = parse_field_range(field, ranges.len()) {

        let &(begin, _) = ranges.get(start).unwrap();
        let &(_, end) = ranges.get(stop-1).unwrap_or(&(text.len(), 0));
        Some(&text[begin..end])
    } else {
        None
    }
}

pub fn get_string_by_range<'a>(delimiter: &Regex, text: &'a str, range: &str) -> Option<&'a str> {
    parse_range(range).and_then(|field| get_string_by_field(delimiter, text, &field))
}

// -> a vector of the matching fields.
pub fn parse_matching_fields(delimiter: &Regex, text: &str, fields: &[FieldRange]) -> Vec<(usize, usize)> {
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

// range: "start..end", end is excluded.
// "0", "0..", "..10", "1..10", etc.
pub fn parse_range(range: &str) -> Option<FieldRange> {
    use self::FieldRange::*;

    if range == ".." {
        return Some(RightInf(0));
    }

    let range_string: Vec<&str> = range.split("..").collect();
    if range_string.is_empty() || range_string.len() > 2 {
        return None;
    }

    let start = range_string.get(0).and_then(|x| x.parse::<i64>().ok());
    let end = range_string.get(1).and_then(|x| x.parse::<i64>().ok());

    if range_string.len() == 1 {
        return if start.is_none() {None} else {Some(Single(start.unwrap()))};
    }

    if start.is_none() && end.is_none() {
        None
    } else if end.is_none() {
        // 1..
        Some(RightInf(start.unwrap()))
    } else if start.is_none() {
        // ..1
        Some(LeftInf(end.unwrap()))
    } else {
        Some(Both(start.unwrap(), end.unwrap()))
    }
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
            let left = if left >= 0 {left} else {length + left};
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

pub fn parse_transform_fields(delimiter: &Regex, text: &str, fields: &[FieldRange]) -> String {
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


#[cfg(test)]
mod test {
    use super::FieldRange::*;
    #[test]
    fn test_parse_range() {
        assert_eq!(super::parse_range("1"), Some(Single(1)));
        assert_eq!(super::parse_range("-1"), Some(Single(-1)));

        assert_eq!(super::parse_range("1.."), Some(RightInf(1)));
        assert_eq!(super::parse_range("-1.."), Some(RightInf(-1)));

        assert_eq!(super::parse_range("..1"), Some(LeftInf(1)));
        assert_eq!(super::parse_range("..-1"), Some(LeftInf(-1)));

        assert_eq!(super::parse_range("1..3"), Some(Both(1, 3)));
        assert_eq!(super::parse_range("-1..-3"), Some(Both(-1, -3)));

        assert_eq!(super::parse_range(".."), Some(RightInf(0)));
        assert_eq!(super::parse_range("a.."), None);
        assert_eq!(super::parse_range("..b"), None);
        assert_eq!(super::parse_range("a..b"), None);
    }

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

    use super::*;
    #[test]
    fn test_get_string_by_field() {
        // delimiter is ","
        let re = Regex::new(",").unwrap();
        let text = "a,b,c,";
        assert_eq!(get_string_by_field(&re, &text, &Single(0)), Some("a"));
        assert_eq!(get_string_by_field(&re, &text, &Single(1)), Some("b"));
        assert_eq!(get_string_by_field(&re, &text, &Single(2)), Some("c"));
        assert_eq!(get_string_by_field(&re, &text, &Single(3)), Some(""));
        assert_eq!(get_string_by_field(&re, &text, &Single(4)), None);
        assert_eq!(get_string_by_field(&re, &text, &Single(5)), None);
        assert_eq!(get_string_by_field(&re, &text, &Single(-1)), Some(""));
        assert_eq!(get_string_by_field(&re, &text, &Single(-2)), Some("c"));
        assert_eq!(get_string_by_field(&re, &text, &Single(-3)), Some("b"));
        assert_eq!(get_string_by_field(&re, &text, &Single(-4)), Some("a"));
        assert_eq!(get_string_by_field(&re, &text, &Single(-5)), None);
        assert_eq!(get_string_by_field(&re, &text, &Single(-1)), Some(""));

        assert_eq!(get_string_by_field(&re, &text, &LeftInf(0)), None);
        assert_eq!(get_string_by_field(&re, &text, &LeftInf(1)), Some("a"));
        assert_eq!(get_string_by_field(&re, &text, &LeftInf(2)), Some("a,b"));
        assert_eq!(get_string_by_field(&re, &text, &LeftInf(3)), Some("a,b,c"));
        assert_eq!(get_string_by_field(&re, &text, &LeftInf(4)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, &text, &LeftInf(5)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, &text, &LeftInf(-5)), None);
        assert_eq!(get_string_by_field(&re, &text, &LeftInf(-4)), None);
        assert_eq!(get_string_by_field(&re, &text, &LeftInf(-3)), Some("a"));
        assert_eq!(get_string_by_field(&re, &text, &LeftInf(-2)), Some("a,b"));
        assert_eq!(get_string_by_field(&re, &text, &LeftInf(-1)), Some("a,b,c"));

        assert_eq!(get_string_by_field(&re, &text, &RightInf(0)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, &text, &RightInf(1)), Some("b,c,"));
        assert_eq!(get_string_by_field(&re, &text, &RightInf(2)), Some("c,"));
        assert_eq!(get_string_by_field(&re, &text, &RightInf(3)), Some(""));
        assert_eq!(get_string_by_field(&re, &text, &RightInf(4)), None);
        assert_eq!(get_string_by_field(&re, &text, &RightInf(5)), None);
        assert_eq!(get_string_by_field(&re, &text, &RightInf(-5)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, &text, &RightInf(-4)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, &text, &RightInf(-3)), Some("b,c,"));
        assert_eq!(get_string_by_field(&re, &text, &RightInf(-2)), Some("c,"));
        assert_eq!(get_string_by_field(&re, &text, &RightInf(-1)), Some(""));

        assert_eq!(get_string_by_field(&re, &text, &Both(0, 0)), None);
        assert_eq!(get_string_by_field(&re, &text, &Both(0, 1)), Some("a"));
        assert_eq!(get_string_by_field(&re, &text, &Both(0, 2)), Some("a,b"));
        assert_eq!(get_string_by_field(&re, &text, &Both(0, 3)), Some("a,b,c"));
        assert_eq!(get_string_by_field(&re, &text, &Both(0, 4)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, &text, &Both(0, 5)), Some("a,b,c,"));
        assert_eq!(get_string_by_field(&re, &text, &Both(1, 5)), Some("b,c,"));
        assert_eq!(get_string_by_field(&re, &text, &Both(2, 5)), Some("c,"));
        assert_eq!(get_string_by_field(&re, &text, &Both(3, 5)), Some(""));
        assert_eq!(get_string_by_field(&re, &text, &Both(4, 5)), None);
        assert_eq!(get_string_by_field(&re, &text, &Both(5, 5)), None);
        assert_eq!(get_string_by_field(&re, &text, &Both(6, 5)), None);
        assert_eq!(get_string_by_field(&re, &text, &Both(1, 3)), Some("b,c"));
        assert_eq!(get_string_by_field(&re, &text, &Both(2, 3)), Some("c"));
        assert_eq!(get_string_by_field(&re, &text, &Both(3, 3)), None);
        assert_eq!(get_string_by_field(&re, &text, &Both(4, 3)), None);
    }
}

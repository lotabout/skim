use regex::Regex;

pub fn regex_match(choice: &str, pattern: &Option<Regex>) -> Option<(usize, usize)> {
    match *pattern {
        Some(ref pat) => {
            let mat = pat.find(choice)?;
            Some((mat.start(), mat.end()))
        }
        None => None,
    }
}

pub fn contains_upper(string: &str) -> bool {
    for ch in string.chars() {
        if ch.is_ascii_uppercase() {
            return true;
        }
    }
    false
}

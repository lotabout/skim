use fuzzy_matcher;
///! score is responsible for calculating the scores of the similarity between
///! the query and the choice.
use regex::Regex;

pub fn fuzzy_match(choice: &str, pattern: &str) -> Option<(i64, Vec<usize>)> {
    if pattern.is_empty() {
        return Some((0, Vec::new()));
    } else if choice.is_empty() {
        return None;
    }

    fuzzy_matcher::skim::fuzzy_indices(choice, pattern)
}

pub fn regex_match(choice: &str, pattern: &Option<Regex>) -> Option<(usize, usize)> {
    match *pattern {
        Some(ref pat) => {
            let mat = pat.find(choice)?;
            Some((mat.start(), mat.end()))
        }
        None => None,
    }
}

// Pattern may appear in sevearl places, return the first and last occurrence
pub fn exact_match(choice: &str, pattern: &str) -> Option<((usize, usize), (usize, usize))> {
    // search from the start
    let start_pos = choice.find(pattern)?;
    let first_occur = (start_pos, start_pos + pattern.len());
    let last_pos = choice.rfind(pattern)?;
    let last_occur = (last_pos, last_pos + pattern.len());
    Some((first_occur, last_occur))
}

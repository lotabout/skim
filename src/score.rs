/// score is responsible for calculating the scores of the similarity between
/// the query and the choice.
///
/// It is modeled after https://github.com/felipesere/icepick.git

use std::cmp::max;
use std::cell::RefCell;
use regex::Regex;

const BONUS_UPPER_MATCH: i64 = 10;
const BONUS_ADJACENCY: i64 = 10;
const BONUS_SEPARATOR: i64 = 20;
const BONUS_CAMEL: i64 = 20;
const PENALTY_CASE_UNMATCHED: i64 = -1;
const PENALTY_LEADING: i64 = -6; // penalty applied for every letter before the first match
const PENALTY_MAX_LEADING: i64 = -18; // maxing penalty for leading letters
const PENALTY_UNMATCHED: i64 = -2;

// judge how many scores the current index should get
fn fuzzy_score(string: &[char], index: usize, pattern: &[char], pattern_idx: usize) -> i64 {
    let mut score = 0;

    let pattern_char = pattern[pattern_idx];
    let cur = string[index];

    if pattern_char.is_uppercase() && cur.is_uppercase() && pattern_char == cur {
        score += BONUS_UPPER_MATCH;
    } else {
        score += PENALTY_CASE_UNMATCHED;
    }

    if index == 0 {
        return score + if cur.is_uppercase() {BONUS_CAMEL} else {0};
    }

    let prev = string[index-1];

    // apply bonus for matches after a separator
    if prev == ' ' || prev == '_' || prev == '-' || prev == '/' || prev == '\\' {
        score += BONUS_SEPARATOR;
    }

    // apply bonus for camelCases
    if prev.is_lowercase() && cur.is_uppercase() {
        score += BONUS_CAMEL;
    }

    if pattern_idx == 0 {
        score += max((index as i64) * PENALTY_LEADING, PENALTY_MAX_LEADING);
    }

    score
}

pub fn fuzzy_match(choice: &[char],
                   pattern: &[char],
                   pattern_lower: &[char]) -> Option<(i64, Vec<usize>)>{
    if pattern.len() == 0 {
        return Some((0, Vec::new()));
    }

    let mut scores = vec![];
    let mut picked = vec![];

    let mut prev_matched_idx = -1; // to ensure that the pushed char are able to match the pattern
    for pattern_idx in 0..pattern_lower.len() {
        let pattern_char = pattern_lower[pattern_idx];
        let vec_cell = RefCell::new(vec![]);
        {
            let mut vec = vec_cell.borrow_mut();
            for (idx, &ch) in choice.iter().enumerate() {
                if ch == pattern_char && (idx as i64) > prev_matched_idx {
                    vec.push((idx, fuzzy_score(choice, idx, pattern, pattern_idx), 0)); // (char_idx, score, vec_idx back_ref)
                }
            }

            if vec.len() <= 0 {
                // not matched
                return None;
            }
            prev_matched_idx = vec[0].0 as i64;
        }
        scores.push(vec_cell);
    }

    for pattern_idx in 0..pattern.len()-1 {
        let cur_row = scores[pattern_idx].borrow();
        let mut next_row = scores[pattern_idx+1].borrow_mut();

        for idx in 0..next_row.len() {
            let (next_char_idx, next_score, _) = next_row[idx];
//(back_ref, &score)
            let (back_ref, score) = cur_row.iter()
                .take_while(|&&(idx, _, _)| idx < next_char_idx)
                .map(|&(char_idx, score, _)| {
                    let adjacent_num = next_char_idx - char_idx - 1;
                    score + next_score + if adjacent_num == 0 {BONUS_ADJACENCY} else {PENALTY_UNMATCHED * adjacent_num as i64}
                })
                .enumerate()
                .max_by_key(|&(_, x)| x)
                .unwrap();

            next_row[idx] = (next_char_idx, score, back_ref);
        }
    }

    let (mut next_col, &(_, score, _)) = scores[pattern.len()-1].borrow().iter().enumerate().max_by_key(|&(_, &x)| x.1).unwrap();
    let mut pattern_idx = pattern.len() as i64 - 1;
    while pattern_idx >= 0 {
        let (idx, _, next) = scores[pattern_idx as usize].borrow()[next_col];
        next_col = next;
        picked.push(idx);
        pattern_idx -= 1;
    }
    picked.reverse();
    Some((score, picked))
}

pub fn regex_match(choice: &str, pattern: &Option<Regex>) -> Option<(usize, usize)>{
    match *pattern {
        Some(ref pat) => {
            let ret = pat.find(choice);
            if ret.is_none() {
                return None;
            }

            let (start, end) = ret.unwrap();
            let first = (&choice[0..start]).chars().count();
            let last = first + (&choice[start..end]).chars().count();
            Some((first, last))
        }
        None => None,
    }
}

#[cfg(test)]
mod test {
    //use super::*;

    //#[test]
    //fn teset_fuzzy_match() {
        //// the score in this test doesn't actually matter, but the index matters.
        //let choice_1 = "1111121";
        //let query_1 = "21";
        //assert_eq!(fuzzy_match(&choice_1, &query_1), Some((-10, vec![5,6])));

        //let choice_2 = "Ca";
        //let query_2 = "ac";
        //assert_eq!(fuzzy_match(&choice_2, &query_2), None);

        //let choice_3 = ".";
        //let query_3 = "s";
        //assert_eq!(fuzzy_match(&choice_3, &query_3), None);

        //let choice_4 = "AaBbCc";
        //let query_4 = "abc";
        //assert_eq!(fuzzy_match(&choice_4, &query_4), Some((53, vec![0,2,4])));
    //}
}

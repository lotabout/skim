/// score is responsible for calculating the scores of the similarity between
/// the query and the choice.
///
/// It is modeled after <https://github.com/felipesere/icepick.git>

use std::cmp::max;
use std::cell::RefCell;
use regex::Regex;
use std::ascii::AsciiExt;

const BONUS_UPPER_MATCH: i64 = 10;
const BONUS_ADJACENCY: i64 = 10;
const BONUS_SEPARATOR: i64 = 8;
const BONUS_CAMEL: i64 = 8;
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
        return score + if cur.is_uppercase() { BONUS_CAMEL } else { 0 };
    }

    let prev = string[index - 1];

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

#[derive(Clone, Copy, Debug)]
struct MatchingStatus {
    pub idx: usize,
    pub score: i64,
    pub final_score: i64,
    pub adj_num: usize,
    pub back_ref: usize,
}

impl MatchingStatus {
    pub fn empty() -> Self {
        MatchingStatus {
            idx: 0,
            score: 0,
            final_score: 0,
            adj_num: 1,
            back_ref: 0,
        }
    }
}

pub fn fuzzy_match(choice: &[char], pattern: &[char]) -> Option<(i64, Vec<usize>)> {
    if pattern.is_empty() {
        return Some((0, Vec::new()));
    }

    let mut scores = vec![];
    let mut picked = vec![];

    let mut prev_matched_idx = -1; // to ensure that the pushed char are able to match the pattern
    for (pattern_idx, pattern_char) in pattern.iter().map(|c| c.to_ascii_lowercase()).enumerate() {
        let vec_cell = RefCell::new(vec![]);
        {
            let mut vec = vec_cell.borrow_mut();
            for (idx, ch) in choice.iter().map(|c| c.to_ascii_lowercase()).enumerate() {
                if ch == pattern_char && (idx as i64) > prev_matched_idx {
                    let score = fuzzy_score(choice, idx, pattern, pattern_idx);
                    vec.push(MatchingStatus {
                        idx: idx,
                        score: score,
                        final_score: score,
                        adj_num: 1,
                        back_ref: 0,
                    });
                }
            }

            if vec.is_empty() {
                // not matched
                return None;
            }
            prev_matched_idx = vec[0].idx as i64;
        }
        scores.push(vec_cell);
    }

    for pattern_idx in 0..pattern.len() - 1 {
        let cur_row = scores[pattern_idx].borrow();
        let mut next_row = scores[pattern_idx + 1].borrow_mut();

        for idx in 0..next_row.len() {
            let next = next_row[idx];
            let prev = if idx > 0 {
                next_row[idx - 1]
            } else {
                MatchingStatus::empty()
            };
            let score_before_idx = prev.final_score - prev.score + next.score
                + PENALTY_UNMATCHED * ((next.idx - prev.idx) as i64)
                - if prev.adj_num == 0 {
                    BONUS_ADJACENCY
                } else {
                    0
                };

            let (back_ref, score, adj_num) = cur_row
                .iter()
                .enumerate()
                .take_while(|&(_, &MatchingStatus { idx, .. })| idx < next.idx)
                .skip_while(|&(_, &MatchingStatus { idx, .. })| idx < prev.idx)
                .map(|(back_ref, cur)| {
                    let adj_num = next.idx - cur.idx - 1;
                    let final_score = cur.final_score + next.score + if adj_num == 0 {
                        BONUS_ADJACENCY
                    } else {
                        PENALTY_UNMATCHED * adj_num as i64
                    };
                    (back_ref, final_score, adj_num)
                })
                .max_by_key(|&(_, x, _)| x)
                .unwrap_or((prev.back_ref, score_before_idx, prev.adj_num));

            next_row[idx] = if idx > 0 && score < score_before_idx {
                MatchingStatus {
                    final_score: score_before_idx,
                    back_ref: prev.back_ref,
                    adj_num: adj_num,
                    ..next
                }
            } else {
                MatchingStatus {
                    final_score: score,
                    back_ref: back_ref,
                    adj_num: adj_num,
                    ..next
                }
            };
        }
    }

    let last_row = scores[pattern.len() - 1].borrow();
    let (mut next_col, &MatchingStatus { final_score, .. }) = last_row
        .iter()
        .enumerate()
        .max_by_key(|&(_, x)| x.final_score)
        .expect("score:fuzzy_match: failed to iterate over last_row");
    let mut pattern_idx = pattern.len() as i64 - 1;
    while pattern_idx >= 0 {
        let status = scores[pattern_idx as usize].borrow()[next_col];
        next_col = status.back_ref;
        picked.push(status.idx);
        pattern_idx -= 1;
    }
    picked.reverse();
    Some((final_score, picked))
}

pub fn regex_match(choice: &str, pattern: &Option<Regex>) -> Option<(usize, usize)> {
    match *pattern {
        Some(ref pat) => {
            let ret = pat.find(choice);
            if ret.is_none() {
                return None;
            }

            let mat = ret.unwrap();
            let (start, end) = (mat.start(), mat.end());
            let first = (&choice[0..start]).chars().count();
            let last = first + (&choice[start..end]).chars().count();
            Some((first, last))
        }
        None => None,
    }
}

// Pattern may appear in sevearl places, return the first and last occurrence
pub fn exact_match(choice: &str, pattern: &str) -> Option<((usize, usize), (usize, usize))> {
    // search from the start
    let start_pos = choice.find(pattern);
    if start_pos.is_none() {
        return None;
    };

    let pattern_len = pattern.chars().count();

    let first_occur = start_pos
        .map(|s| {
            let start = if s == 0 {
                0
            } else {
                (&choice[0..s]).chars().count()
            };
            (start, start + pattern_len)
        })
        .unwrap();

    let last_pos = choice.rfind(pattern);
    if last_pos.is_none() {
        return None;
    };
    let last_occur = last_pos
        .map(|s| {
            let start = if s == 0 {
                0
            } else {
                (&choice[0..s]).chars().count()
            };
            (start, start + pattern_len)
        })
        .unwrap();

    Some((first_occur, last_occur))
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

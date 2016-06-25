/// score is responsible for calculating the scores of the similarity between
/// the query and the choice.
///
/// It is modeled after https://github.com/felipesere/icepick.git
use std::cmp::max;

// return (start, matched_len)
pub fn compute_match_length(choice: &str, query: &str) -> Option<(usize, usize)> {
    if query.len() <= 0 {
        return Some((0, 0));
    }

    let impossible_match = choice.len() + 1;
    let mut matched_start = impossible_match;
    let mut matched_end = impossible_match;

    let mut choice_chars = choice.chars().enumerate().peekable();
    let mut query_chars = query.chars().enumerate().peekable();

    loop {
        if query_chars.peek() == None {
            return Some((matched_start, matched_end - matched_start+1));
        }

        if choice_chars.peek() == None {
            return None;
        }

        let &(idx_choice, c) = choice_chars.peek().unwrap();
        let &(_, q) = query_chars.peek().unwrap();


        if c == q {
            if matched_start == impossible_match { matched_start = idx_choice; }
            let _ = query_chars.next();
        }

        matched_end = idx_choice;
        let _ = choice_chars.next();
    }
}


// The fuzzy search algorith is inspired by https://github.com/forrestthewoods/lib_fts
//
// # Why is new Fuzzy Match algorithm needed?
// 1. fzf use the query to compose regex for non-greedy match. When use for file/path match, it did
//    not work out for the best(e.g. Camel case characters should weight more)
// 2. fzf will only show the `range` be matched, while I think it is not enought sometimes. with
//    this fuzzy_match algorithm we are able to get all matched indics.
//
// # The complexity is O(n^2)
// An intuitive way of fuzzy match is to find all occurences of pattern characters in the target
// string. Then find out all the choice composites and evaluate the score for each. which will take
// O(n!) for each input string. Then with dynamic programing, we can reduce the time to O(n^3) with
// addition O(m*n) space;
//
// I think that O(n^3) may not be acceptable in the use case of fzf-rs. so I made some assumptions
// and reduce the complexity to O(n^2). Note that lib_fts' complexity is O(n).
//
// # The algorithm
// The scoring core is the same to lib_fts. The difference part is that I search more composites
// than lib_fts for a better result.
//
// \  A   B   a   b   C   c
//  \------------------------
// a| 20   x  -6
// b|  x  20+5 x .Y.
// c|
//
// For each matched key(b for example), we need to find all the matched A before and find the best
// matched A that will get us the max score for current b. For example, the one marked as '.Y.' in
// the previous example will search the row before(i.e. 'a') and calculate the max score for '.Y.'.
//
// This process will take O(n^3), because each row will cause O(n^2);
//
// So as shown below:
//       1         2         3        4
//    ...a.........a.........a....b...a....b....
// a
// b                              b1       b2
//
// if we already know that b1 gets max score when we choose a2, we assume that b2 will get max
// score only if 'a' is chosen within [a2..a4]. That means a1 is not searched any more.
//
// The worst case of course is also O(n^3) but normally that won't happan. Of course I wonder
// whether O(n^3) algorithm is acceptable or not.

const BONUS_ADJACENCY: i32 = 5;
const BONUS_SEPARATOR: i32 = 10;
const BONUS_CAMEL: i32 = 10;
const PENALTY_LEADING: i32 = -3; // penalty applied for every letter before the first match
const PENALTY_MAX_LEADING: i32 = -9; // maxing penalty for leading letters
const PENALTY_UNMATCHED: i32 = -1;

// judge how many scores the current index should get
fn fuzzy_score(string: &Vec<char>, index: usize) -> i32 {
    let mut score = 0;
    if index == 0 {
        return BONUS_SEPARATOR + BONUS_CAMEL;
    }

    let prev = string[index-1];
    let cur = string[index];

    // apply bonus for matches after a separator
    if prev == ' ' || prev == '_' || prev == '-' || prev == '/' || prev == '\\' {
        score += BONUS_SEPARATOR;
    }

    // apply bonus for camelCases
    if prev.is_lowercase() && cur.is_uppercase() {
        score += BONUS_CAMEL;
    }

    score
}

pub fn fuzzy_match(choice: &str, pattern: &str) -> Option<(i32, Vec<usize>)>{
    if pattern.len() == 0 {
        return Some((0, Vec::new()));
    }

    let choice_chars: Vec<char> = choice.chars().collect();
    let pattern_chars: Vec<char> = pattern.to_lowercase().chars().collect();

    let mut scores = vec![vec![]];
    let mut picked = vec![];

    // initialize the first row of scores
    for choice_idx in 0..choice.len() {
        if pattern_chars[0] == choice_chars[choice_idx].to_lowercase().next().unwrap() {
            let score = fuzzy_score(&choice_chars, choice_idx) + max(choice_idx as i32 * PENALTY_LEADING, PENALTY_MAX_LEADING);
            scores[0].push((choice_idx, score, 0)); //(char_idx, score, vec_idx back_ref)
        }
    }

    for pattern_idx in 1..pattern.len() {
        scores.push(vec![]);

        if scores[pattern_idx-1].len() <= 0 {
            return None;
        }

        let mut best_idx = 0; // points to previous line
        for choice_idx in 1..choice.len() {
            let mut best_score = -10000;

            if pattern_chars[pattern_idx] != choice_chars[choice_idx].to_lowercase().next().unwrap() {
                continue;
            }

            for (vec_idx, &(idx, prev_score, _)) in scores[pattern_idx-1][best_idx..].iter().enumerate() {
                if idx >= choice_idx {
                    break;
                }

                let mut char_score = fuzzy_score(&choice_chars, choice_idx);
                let adjacent_num = choice_idx - idx - 1;
                char_score += if adjacent_num == 0 {BONUS_ADJACENCY} else {PENALTY_UNMATCHED * adjacent_num as i32};
                char_score += prev_score;

                if char_score > best_score {
                    best_score = char_score;
                    best_idx = vec_idx;
                }
            }

            scores[pattern_idx].push((choice_idx, best_score, best_idx));
        }
    }

    let (mut next_col, &(_, score, _)) = scores[pattern.len()-1].iter().enumerate().max_by_key(|&(_, &x)| x.1).unwrap();
    let mut pattern_idx = pattern.len() as i32 - 1;
    while pattern_idx >= 0 {
        let (idx, _, next) = scores[pattern_idx as usize][next_col];
        next_col = next;
        picked.push(idx);
        pattern_idx -= 1;
    }
    picked.reverse();
    Some((score, picked))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_compute_match_length() {
        let choice_1 = "I am a 中国人.";
        let query_1 = "a人";
        assert_eq!(super::compute_match_length(&choice_1, &query_1), Some((2, 8)));

        let choice_2 = "Choice did not matter";
        let query_2 = "";
        assert_eq!(super::compute_match_length(&choice_2, &query_2), Some((0, 0)));

        let choice_3 = "abcdefg";
        let query_3 = "hi";
        assert_eq!(super::compute_match_length(&choice_3, &query_3), None);

        let choice_4 = "Partial match did not count";
        let query_4 = "PP";
        assert_eq!(compute_match_length(&choice_4, &query_4), None);
    }

    #[test]
    fn teset_fuzzy_match() {
        let choice_4 = "AaBbcc";
        let query_4 = "abc";
        assert_eq!(fuzzy_match(&choice_4, &query_4), Some((28, vec![0,2,4])));
    }
}

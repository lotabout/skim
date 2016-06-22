/// score is responsible for calculating the scores of the similarity between
/// the query and the choice.
///
/// It is modeled after https://github.com/felipesere/icepick.git

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
}

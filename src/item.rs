// An item is line of text that read from `find` command or stdin together with
// the internal states, such as selected or not

use std;
use std::cmp::Ordering;

pub struct Item {
    pub text: String,
    pub selected: bool,
}

impl Item {
    pub fn new(text: String) -> Self {
        Item {
            text: text,
            selected: false,
        }
    }

    pub fn toggle_select(&mut self, selected: Option<bool>) {
        match selected {
            Some(s) => {self.selected = s;}
            None => {self.selected = !self.selected;}
        }
    }
}

pub type Score = (usize, usize); // score (matched-len, start pos)
pub type Range = (usize, usize); // (start, end), end is excluded

#[derive(Eq)]
pub struct MatchedItem {
    pub index: usize,                       // index of current item in items
    pub score: Score,
    pub matched_range_chars: Range,  // range of chars that metched the pattern
}

impl MatchedItem {
    pub fn new(index: usize) -> Self {
        MatchedItem {
            index: index,
            score: (std::usize::MAX, 0),
            matched_range_chars: (0, 0),
        }
    }

    pub fn set_matched_range(&mut self, range: Range) {
        self.matched_range_chars = range;
    }

    pub fn set_score(&mut self, score: Score) {
        self.score = score;
    }
}

impl Ord for MatchedItem {
    fn cmp(&self, other: &MatchedItem) -> Ordering {
        other.score.cmp(&self.score)
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
        self.score == other.score
    }
}

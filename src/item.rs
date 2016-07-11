// An item is line of text that read from `find` command or stdin together with
// the internal states, such as selected or not

use std::cmp::Ordering;

pub struct Item {
    pub text: String,
}

impl Item {
    pub fn new(text: String) -> Self {
        Item {
            text: text,
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

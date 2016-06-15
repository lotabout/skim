// An item is line of text that read from `find` command or stdin together with
// the internal states, such as selected or not

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

pub struct MatchedItem {
    pub index: usize,                       // index of current item in items
    pub rank: [i32; 5],                   // the scores in different criteria
    pub matched_range_bytes: (i32, i32),  // range of bytes that metched the pattern
}

impl MatchedItem {
    pub fn new(index: usize) -> Self {
        MatchedItem {
            index: index,
            rank: [0, 0, 0, 0, 0],
            matched_range_bytes: (0, 0),
        }
    }

    pub fn set_matched_range(&mut self, start: i32, end: i32) {
        self.matched_range_bytes = (start, end);
    }

    pub fn set_rank(&mut self, pos: usize, val: i32) {
        self.rank[pos] = val;
    }
}

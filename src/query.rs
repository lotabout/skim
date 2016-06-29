pub struct Query {
    query: Vec<char>,
    pub index: usize,
    pub pos: usize,
}

impl Query {
    pub fn new() -> Self {
        Query {
            query: Vec::new(),
            index: 0,
            pos: 0,
        }
    }

    pub fn get_query(&self) -> String {
        self.query.iter().cloned().collect::<String>()
    }

    pub fn add_char (&mut self, ch: char) -> bool {
        self.query.insert(self.index, ch);
        self.index += 1;
        self.pos += if ch.len_utf8() > 1 {2} else {1};
        return true;
    }

    pub fn backward_delete_char(&mut self) -> bool{
        if self.index == 0 {
            return false;
        }

        let ch = self.query.remove(self.index-1);
        self.index -= 1;
        self.pos -= if ch.len_utf8() > 1 {2} else {1};
        return true;
    }

    pub fn backward_char(&mut self) -> bool {
        if self.index <= 0 {
            return false;
        }

        match self.query.get(self.index-1) {
            Some(ch) => {
                self.index -= 1;
                self.pos -= if ch.len_utf8() > 1 {2} else {1};
            }
            None => {}
        }
        false
    }
}

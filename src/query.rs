#[derive(Default)]
pub struct Query {
    query: Vec<char>,
    pub index: usize,
    pub pos: usize,
}

impl Query {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn new_with_query(query: &str) -> Self {
        let mut ret = Query::new();
        for ch in query.chars() {
            ret.add_char(ch);
        }
        ret
    }

    pub fn get_query(&self) -> String {
        self.query.iter().cloned().collect::<String>()
    }

    pub fn add_char (&mut self, ch: char) -> bool {
        self.query.insert(self.index, ch);
        self.index += 1;
        self.pos += if ch.len_utf8() > 1 {2} else {1};
        true
    }

    pub fn backward_delete_char(&mut self) -> bool{
        if self.index == 0 { return false; }

        let ch = self.query.remove(self.index-1);
        self.index -= 1;
        self.pos -= if ch.len_utf8() > 1 {2} else {1};
        true
    }

    pub fn backward_char(&mut self) {
        if self.index == 0 { return; }
        if let Some(ch) = self.query.get(self.index-1) {
            self.index -= 1;
            self.pos -= if ch.len_utf8() > 1 {2} else {1};
        }
    }

    pub fn backward_kill_word(&mut self) -> bool {
        let mut modified = false;
        // skip whitespace
        while self.index > 0 {
            if let Some(&' ') = self.query.get(self.index-1) {
                modified = self.backward_delete_char() || modified;
            } else {
                break;
            }
        }

        while self.index > 0 {
            match self.query.get(self.index-1) {
                Some(&ch) if ch != ' ' => {
                    modified = self.backward_delete_char() || modified;
                }
                Some(_) | None => {break;}
            }
        }
        modified
    }

    pub fn backward_word(&mut self) -> bool {
        // skip whitespace
        while self.index > 0 {
            if let Some(&' ') = self.query.get(self.index-1) {
                self.backward_char();
            } else {
                break;
            }
        }

        while self.index > 0 {
            match self.query.get(self.index-1) {
                Some(&ch) if ch != ' ' => { self.backward_char(); }
                Some(_) | None => {break;}
            }
        }
        false
    }

    pub fn beginning_of_line(&mut self) -> bool {
        self.index = 0;
        self.pos = 0;
        false
    }

    // delete char forward
    pub fn delete_char(&mut self) -> bool {
        if self.index == self.query.len() { return false; }

        let _ = self.query.remove(self.index);
        true
    }

    pub fn forward_char(&mut self) {
        if let Some(ch) = self.query.get(self.index) {
            self.index += 1;
            self.pos += if ch.len_utf8() > 1 {2} else {1};
        }
    }

    pub fn forward_word(&mut self) {
        // skip whitespace
        while let Some(_) = self.query.get(self.index) {
            self.forward_char();
        }

        loop {
            match self.query.get(self.index) {
                Some(&ch) if ch != ' ' => self.forward_char(),
                Some(_) | None => break
            }
        }
    }

    pub fn kill_word(&mut self) -> bool {
        let len = self.query.len();
        let mut modified = false;
        // skip whitespace
        while self.index < len {
            if let Some(&' ') = self.query.get(self.index) {
                modified = self.delete_char() || modified;
            } else {
                break;
            }
        }

        while self.index < len {
            match self.query.get(self.index) {
                Some(&ch) if ch != ' ' => { modified = self.delete_char() || modified; }
                Some(_) | None => {break;}
            }
        }
        modified
    }

    pub fn end_of_line(&mut self) -> bool {
        let len = self.query.len();
        while self.index < len {
            self.forward_char();
        }
        false
    }

    pub fn kill_line(&mut self) -> bool {
        if self.index == self.query.len() {return false}
        while self.query.len() > self.index {
            let _ = self.query.pop();
        }
        true
    }

    pub fn line_discard(&mut self) -> bool {
        let mut modified = false;
        while self.index > 0 {
            modified = self.backward_delete_char() || modified;
        }
        modified
    }
}

#[cfg(test)]
mod tests {
    use super::Query;

    #[test]
    fn test_backward_char() {
        // Test that going back from zero does not overflow.
        Query::new().backward_char();
    }
}

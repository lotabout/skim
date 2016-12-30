use std::io::{Write, stdout, Stdout};
use model_new::ClosureType;

pub struct Query {
    cmd_before: Vec<char>,
    cmd_after: Vec<char>,
    query_before: Vec<char>,
    query_after: Vec<char>,
}

impl Query {
    pub fn new(cmd: Option<&str>, query: Option<&str>) -> Self {
        Query {
            cmd_before: cmd.unwrap_or(&"").chars().collect(),
            cmd_after: Vec::new(),
            query_before: query.unwrap_or(&"").chars().collect(),
            query_after: Vec::new(),
        }
    }
    pub fn get_query(&self) -> String {
        self.query_before.iter().cloned().chain(self.query_after.iter().cloned().rev()).collect()
    }

    fn get_before(&self) -> String {
        self.query_before.iter().cloned().collect()
    }

    fn get_after(&self) -> String {
        self.query_after.iter().cloned().rev().collect()
    }

    pub fn get_cmd(&self) -> String {
        self.cmd_before.iter().cloned().chain(self.cmd_after.iter().cloned().rev()).collect()
    }

    pub fn get_print_func(&self) -> ClosureType {
        let before = self.get_before();
        let after = self.get_after();

        Box::new(move |curses| {
            let (h, w) = curses.get_maxyx();

            curses.mv(h - 1, 0);
            curses.printw("> ");
            curses.printw(&before);
            let (y, x) = curses.getyx();
            curses.printw(&after);
            curses.mv(y, x);
        })
    }

//------------------------------------------------------------------------------
// Actions
//
    pub fn act_add_char(&mut self, ch: char) {
        self.query_before.push(ch);
    }

    pub fn act_backward_delete_char(&mut self) {
        self.query_before.pop();
    }

    pub fn act_backward_char(&mut self) {
        self.query_before.pop().map(|ch| {
            self.query_after.push(ch);
        });
    }

    pub fn act_forward_char(&mut self) {
        self.query_after.pop().map(|ch| {
            self.query_before.push(ch);
        });
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_new_query() {
        let query1 = super::Query::new(None);
        assert_eq!(query1.get_query(), "");

        let query2 = super::Query::new(Some("abc"));
        assert_eq!(query2.get_query(), "abc");
    }
}

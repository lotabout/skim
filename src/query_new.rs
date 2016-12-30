use termion::{clear, cursor, terminal_size};
use termion::raw::{RawTerminal, IntoRawMode};
use std::io::{Write, stdout, Stdout};
use model_new::ClosureType;

pub struct Query {
    cmd_before: Vec<char>,
    cmd_after: Vec<char>,
    query_before: Vec<char>,
    query_after: Vec<char>,
    stdout: RawTerminal<Stdout>,
}

impl Query {
    pub fn new(cmd: Option<&str>, query: Option<&str>) -> Self {
        Query {
            cmd_before: cmd.unwrap_or(&"").chars().collect(),
            cmd_after: Vec::new(),
            query_before: query.unwrap_or(&"").chars().collect(),
            query_after: Vec::new(),
            stdout: stdout().into_raw_mode().unwrap(),
        }
    }
    pub fn get_query(&self) -> String {
        self.query_before.iter().cloned().chain(self.query_after.iter().cloned().rev()).collect()
    }

    pub fn get_cmd(&self) -> String {
        self.cmd_before.iter().cloned().chain(self.cmd_after.iter().cloned().rev()).collect()
    }

    pub fn get_print_func(&self) -> ClosureType {
        let (width, height) = terminal_size().unwrap();
        let (width, height) = (width as usize, height as usize);
        let query = self.get_query();

        Box::new(move |stdout| {
            write!(stdout, "{}{}", cursor::Goto(1, height as u16), clear::CurrentLine);
            write!(stdout, "> {}", query);
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

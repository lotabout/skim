use termion::{clear, cursor, terminal_size};
use termion::raw::{RawTerminal, IntoRawMode};
use std::io::{Write, stdout, Stdout};

pub struct Query {
    before: Vec<char>,
    after: Vec<char>,
    stdout: RawTerminal<Stdout>,
}

impl Query {
    pub fn new(query: Option<&str>) -> Self {
        Query {
            before: query.unwrap_or(&"").chars().collect(),
            after: Vec::new(),
            stdout: stdout().into_raw_mode().unwrap(),
        }
    }
    pub fn get_query(&self) -> String {
        self.before.iter().cloned().chain(self.after.iter().cloned().rev()).collect()
    }

    pub fn print_screen(&mut self) {
        let (width, height) = terminal_size().unwrap();
        let (width, height) = (width as usize, height as usize);
        let query = self.get_query();

        write!(self.stdout, "{}{}", cursor::Goto(1, height as u16), clear::CurrentLine);
        write!(self.stdout, "> {}", query);
        self.stdout.flush().unwrap();
    }

//------------------------------------------------------------------------------
// Actions
//
    pub fn act_add_char(&mut self, ch: char) {
        self.before.push(ch);
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

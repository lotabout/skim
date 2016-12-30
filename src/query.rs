use std::io::{Write, stdout, Stdout};
use model::ClosureType;
use getopts;

#[derive(Clone, Copy)]
enum QueryMode {
    CMD,
    QUERY,
}

pub struct Query {
    cmd_before: Vec<char>,
    cmd_after: Vec<char>,
    query_before: Vec<char>,
    query_after: Vec<char>,

    mode: QueryMode,
    cmd: String,
    replstr: String,
}

impl Query {
    pub fn builder() -> Self {
        Query {
            cmd_before: Vec::new(),
            cmd_after: Vec::new(),
            query_before: Vec::new(),
            query_after: Vec::new(),
            mode: QueryMode::QUERY,
            cmd: String::new(),
            replstr: "{}".to_string(),
        }
    }

    // builder
    pub fn cmd(mut self, cmd: &str) -> Self {
        self.cmd = cmd.to_owned();
        self
    }

    pub fn cmd_arg(mut self,arg: &str) -> Self {
        self.cmd_before = arg.chars().collect();
        self
    }

    pub fn query(mut self, query: &str) -> Self {
        self.query_before = query.chars().collect();
        self
    }

    pub fn replstr(mut self, replstr: &str) -> Self {
        self.replstr = replstr.to_owned();
        self
    }

    pub fn mode(mut self, mode: QueryMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn build(self) -> Self {
        self
    }

    pub fn parse_options(&mut self, options: &getopts::Matches) {
        if let Some(cmd) = options.opt_str("c") {
            self.cmd = cmd.clone();
        }

        if let Some(query) = options.opt_str("q") {
            self.query_before = query.chars().collect();
        }

        if let Some(replstr) = options.opt_str("I") {
            self.replstr = replstr.clone();
        }

        if options.opt_present("i") {
            self.mode = QueryMode::CMD;
        }
    }

    pub fn get_query(&self) -> String {
        self.query_before.iter().cloned().chain(self.query_after.iter().cloned().rev()).collect()
    }

    pub fn get_cmd(&self) -> String {
        let arg: String = self.cmd_before.iter().cloned().chain(self.cmd_after.iter().cloned().rev()).collect();
        self.cmd.replace(&self.replstr, &arg)
    }

    fn get_before(&self) -> String {
        match self.mode {
            QueryMode::CMD   => self.cmd_before.iter().cloned().collect(),
            QueryMode::QUERY => self.query_before.iter().cloned().collect(),
        }
    }

    fn get_after(&self) -> String {
        match self.mode {
            QueryMode::CMD   => self.cmd_after.iter().cloned().collect(),
            QueryMode::QUERY => self.query_after.iter().cloned().collect(),
        }
    }

    pub fn get_print_func(&self) -> ClosureType {
        let before = self.get_before();
        let after = self.get_after();
        let mode = self.mode;

        Box::new(move |curses| {
            let (h, w) = curses.get_maxyx();

            match mode {
                QueryMode::CMD   => curses.printw("C"),
                QueryMode::QUERY => curses.printw("Q"),
            }

            curses.printw("> ");
            curses.printw(&before);
            let (y, x) = curses.getyx();
            curses.printw(&after);
            curses.mv(y, x);
        })
    }

    fn get_ref(&mut self) -> (&mut Vec<char>, &mut Vec<char>) {
        match self.mode {
            QueryMode::QUERY => (&mut self.query_before, &mut self.query_after),
            QueryMode::CMD   => (&mut self.cmd_before, &mut self.cmd_after)
        }
    }

//------------------------------------------------------------------------------
// Actions
//
    pub fn act_query_rotate_mode(&mut self) {
        self.mode = match self.mode {
            QueryMode::QUERY => QueryMode::CMD,
            QueryMode::CMD   => QueryMode::QUERY,
        }
    }

    pub fn act_add_char(&mut self, ch: char) {
        let (before, _) = self.get_ref();
        before.push(ch);
    }

    pub fn act_backward_delete_char(&mut self) {
        let (before, _) = self.get_ref();
        before.pop();
    }

    pub fn act_backward_char(&mut self) {
        let (before, after) = self.get_ref();
        before.pop().map(|ch| {
            after.push(ch);
        });
    }

    pub fn act_forward_char(&mut self) {
        let (before, after) = self.get_ref();
        after.pop().map(|ch| {
            before.push(ch);
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

use model::ClosureType;
use getopts;
use curses::*;

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
    base_cmd: String,
    replstr: String,
    query_prompt: String,
    cmd_prompt: String,
}

impl Query {
    pub fn builder() -> Self {
        Query {
            cmd_before: Vec::new(),
            cmd_after: Vec::new(),
            query_before: Vec::new(),
            query_after: Vec::new(),
            mode: QueryMode::QUERY,
            base_cmd: String::new(),
            replstr: "{}".to_string(),
            query_prompt: "> ".to_string(),
            cmd_prompt: "c> ".to_string(),
        }
    }

    pub fn base_cmd(mut self, base_cmd: &str) -> Self {
        self.base_cmd = base_cmd.to_owned();
        self
    }

    // currently they are not used, but will in the future
    #[cfg(test)]
    pub fn query(mut self, query: &str) -> Self {
        self.query_before = query.chars().collect();
        self
    }

    //pub fn cmd(mut self, cmd: &str) -> Self {
        //self.cmd_before = cmd.chars().collect();
        //self
    //}

    pub fn build(self) -> Self {
        self
    }

    pub fn parse_options(&mut self, options: &getopts::Matches) {
        if let Some(base_cmd) = options.opt_str("c") {
            self.base_cmd = base_cmd.clone();
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

        if let Some(query_prompt) = options.opt_str("prompt") {
            self.query_prompt = query_prompt;
        }

        if let Some(cmd_prompt) = options.opt_str("cmd-prompt") {
            self.cmd_prompt = cmd_prompt;
        }
    }

    pub fn get_query(&self) -> String {
        self.query_before.iter().cloned().chain(self.query_after.iter().cloned().rev()).collect()
    }

    pub fn get_cmd(&self) -> String {
        let arg: String = self.cmd_before.iter().cloned().chain(self.cmd_after.iter().cloned().rev()).collect();
        self.base_cmd.replace(&self.replstr, &arg)
    }

    fn get_before(&self) -> String {
        match self.mode {
            QueryMode::CMD   => self.cmd_before.iter().cloned().collect(),
            QueryMode::QUERY => self.query_before.iter().cloned().collect(),
        }
    }

    fn get_after(&self) -> String {
        match self.mode {
            QueryMode::CMD   => self.cmd_after.iter().cloned().rev().collect(),
            QueryMode::QUERY => self.query_after.iter().cloned().rev().collect(),
        }
    }

    pub fn get_print_func(&self) -> ClosureType {
        let before = self.get_before();
        let after = self.get_after();
        let mode = self.mode;
        let cmd_prompt = self.cmd_prompt.clone();
        let query_prompt = self.query_prompt.clone();

        Box::new(move |mut curses| {
            match mode {
                QueryMode::CMD   => {
                    curses.cprint(&cmd_prompt, COLOR_PROMPT, false);
                }
                QueryMode::QUERY => {
                    curses.cprint(&query_prompt, COLOR_PROMPT, false);
                }
            }

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
    pub fn act_query_toggle_interactive(&mut self) {
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
        let _ = before.pop();
    }

    // delete char foraward
    pub fn act_delete_char(&mut self) {
        let (_, after) = self.get_ref();
        let _ = after.pop();
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

    pub fn act_backward_kill_word(&mut self) {
        let (before, _) = self.get_ref();

        // skip whitespace
        while !before.is_empty() && before[before.len()-1].is_whitespace() {
            before.pop();
        }

        // kill word until whitespace
        while !before.is_empty() && !before[before.len()-1].is_whitespace() {
            before.pop();
        }
    }

    pub fn act_kill_word(&mut self) {
        let (_, after) = self.get_ref();

        // kill word until whitespace
        while !after.is_empty() && !after[after.len()-1].is_whitespace() {
            after.pop();
        }
        // skip whitespace
        while !after.is_empty() && after[after.len()-1].is_whitespace() {
            after.pop();
        }
    }

    pub fn act_backward_word(&mut self) {
        let (before, after) = self.get_ref();
        // skip whitespace
        while !before.is_empty() && before[before.len()-1].is_whitespace() {
            before.pop().map(|ch| {
                after.push(ch);
            });
        }

        // backword char until whitespace
        while !before.is_empty() && !before[before.len()-1].is_whitespace() {
            before.pop().map(|ch| {
                after.push(ch);
            });
        }
    }

    pub fn act_forward_word(&mut self) {
        let (before, after) = self.get_ref();
        // backword char until whitespace
        while !after.is_empty() && !after[after.len()-1].is_whitespace() {
            after.pop().map(|ch| {
                before.push(ch);
            });
        }

        // skip whitespace
        while !after.is_empty() && after[after.len()-1].is_whitespace() {
            after.pop().map(|ch| {
                before.push(ch);
            });
        }
    }

    pub fn act_beginning_of_line(&mut self) {
        let (before, after) = self.get_ref();
        while !before.is_empty() {
            before.pop().map(|ch| {
                after.push(ch);
            });
        }
    }

    pub fn act_end_of_line(&mut self) {
        let (before, after) = self.get_ref();
        while !after.is_empty() {
            after.pop().map(|ch| {
                before.push(ch);
            });
        }
    }

    pub fn act_kill_line(&mut self) {
        let (_, after) = self.get_ref();
        after.clear();
    }

    pub fn act_line_discard(&mut self) {
        let (before, _) = self.get_ref();
        before.clear();
    }
}

#[cfg(test)]
mod test {
    use super::Query;

    #[test]
    fn test_new_query() {
        let query1 = Query::builder().query("").build();
        assert_eq!(query1.get_query(), "");

        let query2 = Query::builder().query("abc").build();
        assert_eq!(query2.get_query(), "abc");
    }

    #[test]
    fn test_add_char() {
        let mut query1 = Query::builder().query("").build();
        query1.act_add_char('a');
        assert_eq!(query1.get_query(), "a");
        query1.act_add_char('b');
        assert_eq!(query1.get_query(), "ab");
        query1.act_add_char('中');
        assert_eq!(query1.get_query(), "ab中");
    }

    #[test]
    fn test_backward_delete_char() {
        let mut query = Query::builder().query("AB中c").build();
        assert_eq!(query.get_query(), "AB中c");

        query.act_backward_delete_char();
        assert_eq!(query.get_query(), "AB中");

        query.act_backward_delete_char();
        assert_eq!(query.get_query(), "AB");

        query.act_backward_delete_char();
        assert_eq!(query.get_query(), "A");

        query.act_backward_delete_char();
        assert_eq!(query.get_query(), "");

        query.act_backward_delete_char();
        assert_eq!(query.get_query(), "");
    }
}

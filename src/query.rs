use crate::event::{Event, EventArg, EventHandler, UpdateScreen};
use crate::options::SkimOptions;
use crate::theme::{ColorTheme, DEFAULT_THEME};
use crate::util::escape_single_quote;
use lazy_string_replace::LazyReplace;
use std::mem;
use std::sync::Arc;
use tuikit::prelude::*;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, PartialEq)]
enum QueryMode {
    CMD,
    QUERY,
}

pub struct Query {
    cmd_before: Vec<char>,
    cmd_after: Vec<char>,
    query_before: Vec<char>,
    query_after: Vec<char>,
    yank: Vec<char>,

    mode: QueryMode,
    base_cmd: String,
    replstr: String,
    query_prompt: String,
    cmd_prompt: String,

    theme: Arc<ColorTheme>,
}

#[allow(dead_code)]
impl Query {
    pub fn builder() -> Self {
        Query {
            cmd_before: Vec::new(),
            cmd_after: Vec::new(),
            query_before: Vec::new(),
            query_after: Vec::new(),
            yank: Vec::new(),
            mode: QueryMode::QUERY,
            base_cmd: String::new(),
            replstr: "{}".to_string(),
            query_prompt: "> ".to_string(),
            cmd_prompt: "c> ".to_string(),
            theme: Arc::new(*DEFAULT_THEME),
        }
    }

    pub fn from_options(options: &SkimOptions) -> Self {
        let mut query = Self::builder();
        query.parse_options(options);
        query
    }

    pub fn replace_base_cmd_if_not_set(mut self, base_cmd: &str) -> Self {
        if self.base_cmd == "" {
            self.base_cmd = base_cmd.to_owned();
        }
        self
    }

    pub fn query(mut self, query: &str) -> Self {
        self.query_before = query.chars().collect();
        self
    }

    pub fn theme(mut self, theme: Arc<ColorTheme>) -> Self {
        self.theme = theme;
        self
    }

    pub fn build(self) -> Self {
        self
    }

    fn parse_options(&mut self, options: &SkimOptions) {
        // some options accept multiple values, thus take the last one

        if let Some(base_cmd) = options.cmd {
            self.base_cmd = base_cmd.to_string();
        }

        if let Some(query) = options.query {
            self.query_before = query.chars().collect();
        }

        if let Some(cmd_query) = options.cmd_query {
            self.cmd_before = cmd_query.chars().collect();
        }

        if let Some(replstr) = options.replstr {
            self.replstr = replstr.to_string();
        }

        if options.interactive {
            self.mode = QueryMode::CMD;
        }

        if let Some(query_prompt) = options.prompt {
            self.query_prompt = query_prompt.to_string();
        }

        if let Some(cmd_prompt) = options.cmd_prompt {
            self.cmd_prompt = cmd_prompt.to_string();
        }
    }

    pub fn get_query(&self) -> String {
        self.query_before
            .iter()
            .cloned()
            .chain(self.query_after.iter().cloned().rev())
            .collect()
    }

    pub fn get_cmd(&self) -> String {
        let arg = self.get_cmd_query();

        let out = if arg.is_empty() {
            self.base_cmd.replace(&self.replstr[..], "")
        } else {
            self.base_cmd
                .lazy_replace(&self.replstr[..], format_args!("'{}'", escape_single_quote(&arg)))
                .to_string()
        };
        out
    }

    pub fn get_cmd_query(&self) -> String {
        self.cmd_before
            .iter()
            .cloned()
            .chain(self.cmd_after.iter().cloned().rev())
            .collect()
    }

    fn get_before(&self) -> String {
        match self.mode {
            QueryMode::CMD => self.cmd_before.iter().cloned().collect(),
            QueryMode::QUERY => self.query_before.iter().cloned().collect(),
        }
    }

    fn get_after(&self) -> String {
        match self.mode {
            QueryMode::CMD => self.cmd_after.iter().cloned().rev().collect(),
            QueryMode::QUERY => self.query_after.iter().cloned().rev().collect(),
        }
    }

    fn get_prompt(&self) -> &str {
        match self.mode {
            QueryMode::CMD => &self.cmd_prompt,
            QueryMode::QUERY => &self.query_prompt,
        }
    }

    fn get_ref(&mut self) -> (&mut Vec<char>, &mut Vec<char>) {
        match self.mode {
            QueryMode::QUERY => (&mut self.query_before, &mut self.query_after),
            QueryMode::CMD => (&mut self.cmd_before, &mut self.cmd_after),
        }
    }

    fn save_yank(&mut self, mut yank: Vec<char>, reverse: bool) {
        if yank.is_empty() {
            return;
        }

        self.yank.clear();

        if reverse {
            self.yank.append(&mut yank.into_iter().rev().collect());
        } else {
            self.yank.append(&mut yank);
        }
    }

    //------------------------------------------------------------------------------
    // Actions
    //
    pub fn act_query_toggle_interactive(&mut self) {
        self.mode = match self.mode {
            QueryMode::QUERY => QueryMode::CMD,
            QueryMode::CMD => QueryMode::QUERY,
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
        if let Some(ch) = before.pop() {
            after.push(ch);
        }
    }

    pub fn act_forward_char(&mut self) {
        let (before, after) = self.get_ref();
        if let Some(ch) = after.pop() {
            before.push(ch);
        }
    }

    pub fn act_unix_word_rubout(&mut self) {
        let mut yank = Vec::new();

        {
            let (before, _) = self.get_ref();
            // kill things other than whitespace
            while !before.is_empty() && before[before.len() - 1].is_whitespace() {
                yank.push(before.pop().unwrap());
            }

            // kill word until whitespace
            while !before.is_empty() && !before[before.len() - 1].is_whitespace() {
                yank.push(before.pop().unwrap());
            }
        }

        self.save_yank(yank, true);
    }

    pub fn act_backward_kill_word(&mut self) {
        let mut yank = Vec::new();

        {
            let (before, _) = self.get_ref();
            // kill things other than alphanumeric
            while !before.is_empty() && !before[before.len() - 1].is_alphanumeric() {
                yank.push(before.pop().unwrap());
            }

            // kill word until whitespace (not alphanumeric)
            while !before.is_empty() && before[before.len() - 1].is_alphanumeric() {
                yank.push(before.pop().unwrap());
            }
        }

        self.save_yank(yank, true);
    }

    pub fn act_kill_word(&mut self) {
        let mut yank = Vec::new();

        {
            let (_, after) = self.get_ref();

            // kill non alphanumeric
            while !after.is_empty() && !after[after.len() - 1].is_alphanumeric() {
                yank.push(after.pop().unwrap());
            }
            // kill alphanumeric
            while !after.is_empty() && after[after.len() - 1].is_alphanumeric() {
                yank.push(after.pop().unwrap());
            }
        }
        self.save_yank(yank, false);
    }

    pub fn act_backward_word(&mut self) {
        let (before, after) = self.get_ref();
        // skip whitespace
        while !before.is_empty() && !before[before.len() - 1].is_alphanumeric() {
            if let Some(ch) = before.pop() {
                after.push(ch);
            }
        }

        // backword char until whitespace
        while !before.is_empty() && before[before.len() - 1].is_alphanumeric() {
            if let Some(ch) = before.pop() {
                after.push(ch);
            }
        }
    }

    pub fn act_forward_word(&mut self) {
        let (before, after) = self.get_ref();
        // backword char until whitespace
        // skip whitespace
        while !after.is_empty() && after[after.len() - 1].is_whitespace() {
            if let Some(ch) = after.pop() {
                before.push(ch);
            }
        }

        while !after.is_empty() && !after[after.len() - 1].is_whitespace() {
            if let Some(ch) = after.pop() {
                before.push(ch);
            }
        }
    }

    pub fn act_beginning_of_line(&mut self) {
        let (before, after) = self.get_ref();
        while !before.is_empty() {
            if let Some(ch) = before.pop() {
                after.push(ch);
            }
        }
    }

    pub fn act_end_of_line(&mut self) {
        let (before, after) = self.get_ref();
        while !after.is_empty() {
            if let Some(ch) = after.pop() {
                before.push(ch);
            }
        }
    }

    pub fn act_kill_line(&mut self) {
        let after = mem::replace(&mut self.query_after, Vec::new());
        self.save_yank(after, false);
    }

    pub fn act_line_discard(&mut self) {
        let before = mem::replace(&mut self.query_before, Vec::new());
        self.query_before = Vec::new();
        self.save_yank(before, false);
    }

    pub fn act_yank(&mut self) {
        let yank = mem::replace(&mut self.yank, Vec::new());
        for &c in &yank {
            self.act_add_char(c);
        }
        let _ = mem::replace(&mut self.yank, yank);
    }

    fn query_changed(
        &self,
        mode: QueryMode,
        query_before_len: usize,
        query_after_len: usize,
        cmd_before_len: usize,
        cmd_after_len: usize,
    ) -> bool {
        self.mode != mode
            || self.query_before.len() != query_before_len
            || self.query_after.len() != query_after_len
            || self.cmd_before.len() != cmd_before_len
            || self.cmd_after.len() != cmd_after_len
    }
}

impl EventHandler for Query {
    fn accept_event(&self, event: Event) -> bool {
        use crate::event::Event::*;
        match event {
            EvActDeleteCharEOF => !self.get_query().is_empty(),
            EvActAddChar
            | EvActBackwardChar
            | EvActBackwardDeleteChar
            | EvActBackwardKillWord
            | EvActBackwardWord
            | EvActBeginningOfLine
            | EvActDeleteChar
            | EvActEndOfLine
            | EvActForwardChar
            | EvActForwardWord
            | EvActKillLine
            | EvActKillWord
            | EvActNextHistory
            | EvActPreviousHistory
            | EvActUnixLineDiscard
            | EvActUnixWordRubout
            | EvActYank
            | EvActToggleInteractive => true,
            _ => false,
        }
    }

    fn handle(&mut self, event: Event, arg: &EventArg) -> UpdateScreen {
        use crate::event::Event::*;

        let mode = self.mode;
        let query_before_len = self.query_before.len();
        let query_after_len = self.query_after.len();
        let cmd_before_len = self.cmd_before.len();
        let cmd_after_len = self.cmd_after.len();

        match event {
            EvActAddChar => {
                let ch: char = *arg.downcast_ref().expect("EvActAddChar: failed to get argument");
                self.act_add_char(ch);
            }

            EvActDeleteChar | EvActDeleteCharEOF => {
                self.act_delete_char();
            }

            EvActBackwardChar => {
                self.act_backward_char();
            }

            EvActBackwardDeleteChar => {
                self.act_backward_delete_char();
            }

            EvActBackwardKillWord => {
                self.act_backward_kill_word();
            }

            EvActBackwardWord => {
                self.act_backward_word();
            }

            EvActBeginningOfLine => {
                self.act_beginning_of_line();
            }

            EvActEndOfLine => {
                self.act_end_of_line();
            }

            EvActForwardChar => {
                self.act_forward_char();
            }

            EvActForwardWord => {
                self.act_forward_word();
            }

            EvActKillLine => {
                self.act_kill_line();
            }

            EvActKillWord => {
                self.act_kill_word();
            }

            EvActNextHistory | EvActPreviousHistory => {
                unimplemented!();
            }

            EvActUnixLineDiscard => {
                self.act_line_discard();
            }

            EvActUnixWordRubout => {
                self.act_unix_word_rubout();
            }

            EvActYank => {
                self.act_yank();
            }

            EvActToggleInteractive => {
                self.act_query_toggle_interactive();
            }

            _ => {}
        }

        if self.query_changed(mode, query_before_len, query_after_len, cmd_before_len, cmd_after_len) {
            UpdateScreen::REDRAW
        } else {
            UpdateScreen::DONT_REDRAW
        }
    }
}

impl Draw for Query {
    fn draw(&self, canvas: &mut dyn Canvas) -> Result<()> {
        canvas.clear()?;
        let before = self.get_before();
        let after = self.get_after();
        let prompt = self.get_prompt();

        let prompt_width = canvas.print_with_attr(0, 0, prompt, self.theme.prompt())?;
        let before_width = canvas.print_with_attr(0, prompt_width, &before, self.theme.query())?;
        let col = prompt_width + before_width;
        canvas.print_with_attr(0, col, &after, self.theme.query())?;
        canvas.set_cursor(0, col)?;
        canvas.show_cursor(true)?;
        Ok(())
    }

    fn size_hint(&self) -> (Option<usize>, Option<usize>) {
        let before = self.get_before();
        let after = self.get_after();
        let prompt = self.get_prompt();
        (Some(prompt.width() + before.width() + after.width() + 1), None)
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

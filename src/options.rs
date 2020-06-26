use std::rc::Rc;

use derive_builder::Builder;

use crate::{CaseMatching, FuzzyAlgorithm, MatchEngineFactory};

#[derive(Builder)]
#[builder(build_fn(name = "final_build"))]
#[builder(default)]
pub struct SkimOptions<'a> {
    pub bind: Vec<&'a str>,
    pub multi: bool,
    pub prompt: Option<&'a str>,
    pub cmd_prompt: Option<&'a str>,
    pub expect: Option<String>,
    pub tac: bool,
    pub nosort: bool,
    pub tiebreak: Option<String>,
    pub ansi: bool,
    pub exact: bool,
    pub cmd: Option<&'a str>,
    pub interactive: bool,
    pub query: Option<&'a str>,
    pub cmd_query: Option<&'a str>,
    pub regex: bool,
    pub delimiter: Option<&'a str>,
    pub nth: Option<&'a str>,
    pub with_nth: Option<&'a str>,
    pub replstr: Option<&'a str>,
    pub color: Option<&'a str>,
    pub margin: Option<&'a str>,
    pub no_height: bool,
    pub min_height: Option<&'a str>,
    pub height: Option<&'a str>,
    pub preview: Option<&'a str>,
    pub preview_window: Option<&'a str>,
    pub reverse: bool,
    pub read0: bool,
    pub print0: bool,
    pub tabstop: Option<&'a str>,
    pub print_query: bool,
    pub print_cmd: bool,
    pub print_score: bool,
    pub no_hscroll: bool,
    pub no_mouse: bool,
    pub inline_info: bool,
    pub header: Option<&'a str>,
    pub header_lines: usize,
    pub layout: &'a str,
    pub filter: &'a str,
    pub algorithm: FuzzyAlgorithm,
    pub case: CaseMatching,
    pub engine_factory: Option<Rc<dyn MatchEngineFactory>>,
    pub query_history: &'a [String],
    pub cmd_history: &'a [String],
}

impl<'a> Default for SkimOptions<'a> {
    fn default() -> Self {
        Self {
            bind: vec![],
            multi: false,
            prompt: Some("> "),
            cmd_prompt: Some("c> "),
            expect: None,
            tac: false,
            nosort: false,
            tiebreak: None,
            ansi: false,
            exact: false,
            cmd: None,
            interactive: false,
            query: None,
            cmd_query: None,
            regex: false,
            delimiter: None,
            nth: None,
            with_nth: None,
            replstr: Some("{}"),
            color: None,
            margin: Some("0,0,0,0"),
            no_height: false,
            min_height: Some("10"),
            height: Some("100%"),
            preview: None,
            preview_window: Some("right:50%"),
            reverse: false,
            read0: false,
            print0: false,
            tabstop: None,
            print_query: false,
            print_cmd: false,
            print_score: false,
            no_hscroll: false,
            no_mouse: false,
            inline_info: false,
            header: None,
            header_lines: 0,
            layout: "",
            filter: "",
            algorithm: FuzzyAlgorithm::default(),
            case: CaseMatching::default(),
            engine_factory: None,
            query_history: &[],
            cmd_history: &[],
        }
    }
}

impl<'a> SkimOptionsBuilder<'a> {
    pub fn build(&mut self) -> Result<SkimOptions<'a>, String> {
        if let Some(true) = self.no_height {
            self.height = Some(Some("100%"));
        }

        if let Some(true) = self.reverse {
            self.layout = Some("reverse");
        }

        self.final_build()
    }
}

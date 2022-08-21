use std::rc::Rc;

use derive_builder::Builder;

use crate::helper::item_reader::SkimItemReader;
use crate::reader::CommandCollector;
use crate::{CaseMatching, FuzzyAlgorithm, MatchEngineFactory, Selector};
use std::cell::RefCell;

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
    pub exact: bool,
    pub cmd: Option<&'a str>,
    pub interactive: bool,
    pub query: Option<&'a str>,
    pub cmd_query: Option<&'a str>,
    pub regex: bool,
    pub delimiter: Option<&'a str>,
    pub replstr: Option<&'a str>,
    pub color: Option<&'a str>,
    pub margin: Option<&'a str>,
    pub no_height: bool,
    pub no_clear: bool,
    pub min_height: Option<&'a str>,
    pub height: Option<&'a str>,
    pub preview: Option<&'a str>,
    pub preview_window: Option<&'a str>,
    pub reverse: bool,
    pub tabstop: Option<&'a str>,
    pub no_hscroll: bool,
    pub no_mouse: bool,
    pub inline_info: bool,
    pub header: Option<&'a str>,
    pub header_lines: usize,
    pub layout: &'a str,
    pub algorithm: FuzzyAlgorithm,
    pub case: CaseMatching,
    pub engine_factory: Option<Rc<dyn MatchEngineFactory>>,
    pub query_history: &'a [String],
    pub cmd_history: &'a [String],
    pub cmd_collector: Rc<RefCell<dyn CommandCollector>>,
    pub keep_right: bool,
    pub skip_to_pattern: &'a str,
    pub select1: bool,
    pub exit0: bool,
    pub sync: bool,
    pub selector: Option<Rc<dyn Selector>>,
    pub no_clear_if_empty: bool,
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
            exact: false,
            cmd: None,
            interactive: false,
            query: None,
            cmd_query: None,
            regex: false,
            delimiter: None,
            replstr: Some("{}"),
            color: None,
            margin: Some("0,0,0,0"),
            no_height: false,
            no_clear: false,
            min_height: Some("10"),
            height: Some("100%"),
            preview: None,
            preview_window: Some("right:50%"),
            reverse: false,
            tabstop: None,
            no_hscroll: false,
            no_mouse: false,
            inline_info: false,
            header: None,
            header_lines: 0,
            layout: "",
            algorithm: FuzzyAlgorithm::default(),
            case: CaseMatching::default(),
            engine_factory: None,
            query_history: &[],
            cmd_history: &[],
            cmd_collector: Rc::new(RefCell::new(SkimItemReader::new(Default::default()))),
            keep_right: false,
            skip_to_pattern: "",
            select1: false,
            exit0: false,
            sync: false,
            selector: None,
            no_clear_if_empty: false,
        }
    }
}

impl<'a> SkimOptionsBuilder<'a> {
    pub fn build(&mut self) -> Result<SkimOptions<'a>, SkimOptionsBuilderError> {
        if let Some(true) = self.no_height {
            self.height = Some(Some("100%"));
        }

        if let Some(true) = self.reverse {
            self.layout = Some("reverse");
        }

        self.final_build()
    }
}

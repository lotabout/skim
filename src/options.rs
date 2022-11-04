use crate::helper::item_reader::SkimItemReader;
use crate::item::RankCriteria;
use crate::reader::CommandCollector;
use crate::{CaseMatching, FuzzyAlgorithm, Layout, MatchEngineFactory, Selector};
use derive_builder::Builder;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Builder)]
#[builder(build_fn(name = "final_build"))]
#[builder(default)]
pub struct SkimOptions<'a> {
    pub bind: Vec<&'a str>,
    pub multi: bool,
    pub prompt: Option<&'a str>,
    pub cmd_prompt: Option<&'a str>,
    pub expect: Vec<&'a str>,
    pub tac: bool,
    pub nosort: bool,
    pub tiebreak: Vec<RankCriteria>,
    pub exact: bool,
    pub cmd: Option<&'a str>,
    pub interactive: bool,
    pub query: Option<&'a str>,
    pub cmd_query: Option<&'a str>,
    pub regex: bool,
    pub delimiter: &'a str,
    pub replstr: Option<&'a str>,
    pub color: Vec<&'a str>,
    pub margin: Vec<&'a str>,
    pub no_height: bool,
    pub no_clear: bool,
    pub no_clear_start: bool,
    pub min_height: Option<&'a str>,
    pub height: Option<&'a str>,
    pub preview: Option<&'a str>,
    pub preview_window: Option<&'a str>,
    pub reverse: bool,
    pub tabstop: Option<usize>,
    pub no_hscroll: bool,
    pub no_mouse: bool,
    pub inline_info: bool,
    pub header: Option<&'a str>,
    pub header_lines: usize,
    pub layout: Layout,
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
            expect: vec![],
            tac: false,
            nosort: false,
            tiebreak: vec![],
            exact: false,
            cmd: None,
            interactive: false,
            query: None,
            cmd_query: None,
            regex: false,
            delimiter: "",
            replstr: Some("{}"),
            color: vec![],
            margin: vec!["0"; 4],
            no_height: false,
            no_clear: false,
            no_clear_start: false,
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
            layout: Layout::Default,
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
            self.layout = Some(Layout::Reverse);
        }

        self.final_build()
    }
}

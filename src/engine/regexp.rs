use std::fmt::{Display, Error, Formatter};
use std::sync::Arc;

use regex::Regex;

use crate::engine::util::regex_match;
use crate::item::RankBuilder;
use crate::{CaseMatching, MatchEngine};
use crate::{MatchRange, MatchResult, SkimItem};
use std::cmp::min;

//------------------------------------------------------------------------------
// Regular Expression engine
#[derive(Debug)]
pub struct RegexEngine {
    query_regex: Option<Regex>,
    rank_builder: Arc<RankBuilder>,
}

impl RegexEngine {
    pub fn builder(query: &str, case: CaseMatching) -> Self {
        let mut query_builder = String::new();

        match case {
            CaseMatching::Respect => {}
            CaseMatching::Ignore => query_builder.push_str("(?i)"),
            CaseMatching::Smart => {}
        }

        query_builder.push_str(query);

        RegexEngine {
            query_regex: Regex::new(&query_builder).ok(),
            rank_builder: Default::default(),
        }
    }

    pub fn rank_builder(mut self, rank_builder: Arc<RankBuilder>) -> Self {
        self.rank_builder = rank_builder;
        self
    }

    pub fn build(self) -> Self {
        self
    }
}

impl MatchEngine for RegexEngine {
    fn match_item(&self, item: Arc<dyn SkimItem>) -> Option<MatchResult> {
        let mut matched_result = None;
        let item_text = item.text();
        let default_range = [(0, item_text.len())];
        for &(start, end) in item.get_matching_ranges().unwrap_or(&default_range) {
            let start = min(start, item_text.len());
            let end = min(end, item_text.len());
            if self.query_regex.is_none() {
                matched_result = Some((0, 0));
                break;
            }

            matched_result =
                regex_match(&item_text[start..end], &self.query_regex).map(|(s, e)| (s + start, e + start));

            if matched_result.is_some() {
                break;
            }
        }

        let (begin, end) = matched_result?;
        let score = (end - begin) as i32;
        let item_len = item_text.len();

        Some(MatchResult {
            rank: self.rank_builder.build_rank(score, begin, end, item_len),
            matched_range: MatchRange::ByteRange(begin, end),
        })
    }
}

impl Display for RegexEngine {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(
            f,
            "(Regex: {})",
            self.query_regex
                .as_ref()
                .map_or("".to_string(), |re| re.as_str().to_string())
        )
    }
}

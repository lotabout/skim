use crate::engine::util::{contains_upper, regex_match};
use crate::item::RankBuilder;
use crate::{CaseMatching, MatchEngine, MatchRange, MatchResult, SkimItem};
use regex::{escape, Regex};
use std::cmp::min;
use std::fmt::{Display, Error, Formatter};
use std::sync::Arc;

//------------------------------------------------------------------------------
// Exact engine
#[derive(Debug, Copy, Clone, Default)]
pub struct ExactMatchingParam {
    pub prefix: bool,
    pub postfix: bool,
    pub inverse: bool,
    pub case: CaseMatching,
    __non_exhaustive: bool,
}

#[derive(Debug)]
pub struct ExactEngine {
    #[allow(dead_code)]
    query: String,
    query_regex: Option<Regex>,
    rank_builder: Arc<RankBuilder>,
    inverse: bool,
}

impl ExactEngine {
    pub fn builder(query: &str, param: ExactMatchingParam) -> Self {
        let case_sensitive = match param.case {
            CaseMatching::Respect => true,
            CaseMatching::Ignore => false,
            CaseMatching::Smart => contains_upper(query),
        };

        let mut query_builder = String::new();
        if !case_sensitive {
            query_builder.push_str("(?i)");
        }

        if param.prefix {
            query_builder.push('^');
        }

        query_builder.push_str(&escape(query));

        if param.postfix {
            query_builder.push('$');
        }

        let query_regex = if query.is_empty() {
            None
        } else {
            Regex::new(&query_builder).ok()
        };

        ExactEngine {
            query: query.to_string(),
            query_regex,
            rank_builder: Default::default(),
            inverse: param.inverse,
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

impl MatchEngine for ExactEngine {
    fn match_item(&self, item: &dyn SkimItem) -> Option<MatchResult> {
        let item_text = item.text();
        let default_range = [(0, item_text.len())];
        let matched_result = item
            .get_matching_ranges()
            .unwrap_or(&default_range)
            .iter()
            .find_map(|(start, end)| {
                let start = min(*start, item_text.len());
                let end = min(*end, item_text.len());
                if self.query_regex.is_none() {
                    return Some((0, 0));
                }

                let res = regex_match(&item_text[start..end], &self.query_regex).map(|(s, e)| (s + start, e + start));

                if self.inverse {
                    res.xor(Some((0, 0)))
                } else {
                    res
                }
            });

        let (begin, end) = matched_result?;
        let score = (end - begin) as i32;
        let item_len = item_text.len();
        Some(MatchResult {
            rank: self.rank_builder.build_rank(score, begin, end, item_len),
            matched_range: MatchRange::ByteRange(begin, end),
        })
    }
}

impl Display for ExactEngine {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "(Exact|{}{})", if self.inverse { "!" } else { "" }, {
            if let Some(regex) = &self.query_regex {
                regex.as_str()
            } else {
                self.query.as_str()
            }
        })
    }
}

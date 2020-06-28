use crate::engine::util::{contains_upper, regex_match};
use crate::item::{MatchedItem, MatchedRange, Rank};
use crate::{CaseMatching, MatchEngine, SkimItem};
use regex::{escape, Regex};
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
    query: String,
    query_regex: Option<Regex>,
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
            query_builder.push_str("^");
        }

        query_builder.push_str(&escape(query));

        if param.postfix {
            query_builder.push_str("$");
        }

        let query_regex = if query.is_empty() {
            None
        } else {
            Regex::new(&query_builder).ok()
        };

        ExactEngine {
            query: query.to_string(),
            query_regex,
            inverse: param.inverse,
        }
    }

    pub fn build(self) -> Self {
        self
    }
}

impl MatchEngine for ExactEngine {
    fn match_item(&self, item: Arc<dyn SkimItem>) -> Option<MatchedItem> {
        let mut matched_result = None;
        let item_text = item.text();
        let default_range = [(0, item_text.len())];
        for &(start, end) in item.get_matching_ranges().unwrap_or(&default_range) {
            if self.query_regex.is_none() {
                matched_result = Some((0, 0));
                break;
            }

            matched_result =
                regex_match(&item_text[start..end], &self.query_regex).map(|(s, e)| (s + start, e + start));

            if self.inverse {
                matched_result = matched_result.xor(Some((0, 0)))
            }

            if matched_result.is_some() {
                break;
            }
        }

        let (begin, end) = matched_result?;
        let score = (end - begin) as i64;
        let rank = Rank {
            score: -score,
            begin: begin as i64,
            end: end as i64,
        };

        Some(
            MatchedItem::builder(item)
                .rank(rank)
                .matched_range(MatchedRange::ByteRange(begin, end))
                .build(),
        )
    }
}

impl Display for ExactEngine {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(
            f,
            "(Exact|{}{})",
            if self.inverse { "!" } else { "" },
            self.query_regex.as_ref().map(|x| x.as_str()).unwrap_or("")
        )
    }
}

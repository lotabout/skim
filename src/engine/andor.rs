use std::fmt::{Display, Error, Formatter};
use std::sync::Arc;

use crate::item::{MatchedItem, MatchedRange};
use crate::{MatchEngine, SkimItem};

//------------------------------------------------------------------------------
// OrEngine, a combinator
pub struct OrEngine {
    engines: Vec<Box<dyn MatchEngine>>,
}

impl OrEngine {
    pub fn builder() -> Self {
        Self { engines: vec![] }
    }

    pub fn engines(mut self, mut engines: Vec<Box<dyn MatchEngine>>) -> Self {
        self.engines.append(&mut engines);
        self
    }

    pub fn build(self) -> Self {
        self
    }
}

impl MatchEngine for OrEngine {
    fn match_item(&self, item: Arc<dyn SkimItem>) -> Option<MatchedItem> {
        for engine in &self.engines {
            let result = engine.match_item(Arc::clone(&item));
            if result.is_some() {
                return result;
            }
        }

        None
    }
}

impl Display for OrEngine {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(
            f,
            "(Or: {})",
            self.engines
                .iter()
                .map(|e| format!("{}", e))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

//------------------------------------------------------------------------------
// AndEngine, a combinator
pub struct AndEngine {
    engines: Vec<Box<dyn MatchEngine>>,
}

impl AndEngine {
    pub fn builder() -> Self {
        Self { engines: vec![] }
    }

    pub fn engines(mut self, mut engines: Vec<Box<dyn MatchEngine>>) -> Self {
        self.engines.append(&mut engines);
        self
    }

    pub fn build(self) -> Self {
        self
    }

    fn merge_matched_items(&self, items: Vec<MatchedItem>) -> MatchedItem {
        let rank = items[0].rank;
        let item = Arc::clone(&items[0].item);
        let mut ranges = vec![];
        for item in items {
            match item.matched_range {
                Some(MatchedRange::ByteRange(..)) => {
                    ranges.extend(item.range_char_indices().unwrap());
                }
                Some(MatchedRange::Chars(vec)) => {
                    ranges.extend(vec.iter());
                }
                _ => {}
            }
        }

        ranges.sort();
        ranges.dedup();
        MatchedItem::builder(item)
            .rank(rank)
            .matched_range(MatchedRange::Chars(ranges))
            .build()
    }
}

impl MatchEngine for AndEngine {
    fn match_item(&self, item: Arc<dyn SkimItem>) -> Option<MatchedItem> {
        // mock
        let mut results = vec![];
        for engine in &self.engines {
            let result = engine.match_item(Arc::clone(&item))?;
            results.push(result);
        }

        if results.is_empty() {
            None
        } else {
            Some(self.merge_matched_items(results))
        }
    }
}

impl Display for AndEngine {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(
            f,
            "(And: {})",
            self.engines
                .iter()
                .map(|e| format!("{}", e))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

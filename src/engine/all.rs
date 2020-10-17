use std::fmt::{Display, Error, Formatter};
use std::sync::Arc;

use crate::item::RankBuilder;
use crate::{MatchEngine, MatchRange, MatchResult, SkimItem};

//------------------------------------------------------------------------------
#[derive(Debug)]
pub struct MatchAllEngine {
    rank_builder: Arc<RankBuilder>,
}

impl MatchAllEngine {
    pub fn builder() -> Self {
        Self {
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

impl MatchEngine for MatchAllEngine {
    fn match_item(&self, item: Arc<dyn SkimItem>) -> Option<MatchResult> {
        let item_len = item.text().len();
        Some(MatchResult {
            rank: self.rank_builder.build_rank(0, 0, 0, item_len),
            matched_range: MatchRange::ByteRange(0, 0),
        })
    }
}

impl Display for MatchAllEngine {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "Noop")
    }
}

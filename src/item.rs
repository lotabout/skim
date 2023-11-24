///! An item is line of text that read from `find` command or stdin together with
///! the internal states, such as selected or not
use std::cmp::min;
use std::default::Default;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::spinlock::{SpinLock, SpinLockGuard};
use crate::{Arc, MatchRange, Rank, SkimItem, SkimItemPool};

//------------------------------------------------------------------------------

#[derive(Debug)]
pub struct RankBuilder {
    criterion: Vec<RankCriteria>,
}

impl Default for RankBuilder {
    fn default() -> Self {
        Self {
            criterion: vec![RankCriteria::Score, RankCriteria::Begin, RankCriteria::End],
        }
    }
}

impl RankBuilder {
    pub fn new(mut criterion: Vec<RankCriteria>) -> Self {
        if !criterion.contains(&RankCriteria::Score) && !criterion.contains(&RankCriteria::NegScore) {
            criterion.insert(0, RankCriteria::Score);
        }

        criterion.dedup();
        Self { criterion }
    }

    /// score: the greater the better
    pub fn build_rank(&self, score: i32, begin: usize, end: usize, length: usize) -> Rank {
        let mut rank = [0; 4];
        let begin = begin as i32;
        let end = end as i32;
        let length = length as i32;

        for (index, criteria) in self.criterion.iter().take(4).enumerate() {
            let value = match criteria {
                RankCriteria::Score => -score,
                RankCriteria::Begin => begin,
                RankCriteria::End => end,
                RankCriteria::NegScore => score,
                RankCriteria::NegBegin => -begin,
                RankCriteria::NegEnd => -end,
                RankCriteria::Length => length,
                RankCriteria::NegLength => -length,
            };

            rank[index] = value;
        }

        rank
    }
}

//------------------------------------------------------------------------------
#[derive(Clone)]
pub struct MatchedItem {
    pub item: Arc<dyn SkimItem>,
    pub rank: Rank,
    pub matched_range: Option<MatchRange>,
    // range of chars that matched the pattern
    pub item_idx: u32,
}

impl MatchedItem {}

use std::cmp::Ordering as CmpOrd;
use crate::chunklist::{Chunk, ChunkList};

impl PartialEq for MatchedItem {
    fn eq(&self, other: &Self) -> bool {
        self.rank.eq(&other.rank)
    }
}

impl std::cmp::Eq for MatchedItem {}

impl PartialOrd for MatchedItem {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrd> {
        self.rank.partial_cmp(&other.rank)
    }
}

impl Ord for MatchedItem {
    fn cmp(&self, other: &Self) -> CmpOrd {
        self.rank.cmp(&other.rank)
    }
}

//------------------------------------------------------------------------------
const ITEM_POOL_CAPACITY: usize = 1024;

pub struct ItemPool {
    length: AtomicUsize,
    pool: Arc<ChunkList<Arc<dyn SkimItem>>>,
    /// number of items that was `take`n
    taken: AtomicUsize,

    /// reverse first N lines as header
    reserved_items: SpinLock<Vec<Arc<dyn SkimItem>>>,
    lines_to_reserve: usize,
}

impl ItemPool {
    pub fn new() -> Self {
        Self {
            length: AtomicUsize::new(0),
            pool: Arc::new(ChunkList::new()),
            taken: AtomicUsize::new(0),
            reserved_items: SpinLock::new(Vec::new()),
            lines_to_reserve: 0,
        }
    }

    pub fn lines_to_reserve(mut self, lines_to_reserve: usize) -> Self {
        self.lines_to_reserve = lines_to_reserve;
        self
    }

    pub fn len(&self) -> usize {
        self.length.load(Ordering::SeqCst)
    }

    pub fn num_not_taken(&self) -> usize {
        self.length.load(Ordering::SeqCst) - self.taken.load(Ordering::SeqCst)
    }

    pub fn num_taken(&self) -> usize {
        self.taken.load(Ordering::SeqCst)
    }

    pub fn clear(&self) {
        self.pool.clear();
        let mut header_items = self.reserved_items.lock();
        header_items.clear();
        self.taken.store(0, Ordering::SeqCst);
        self.length.store(0, Ordering::SeqCst);
    }

    pub fn reset(&self) {
        // lock to ensure consistency
        self.taken.store(0, Ordering::SeqCst);
    }

    /// append the items and return the new_size of the pool
    pub fn append(&self, mut items: Vec<Arc<dyn SkimItem>>) -> usize {
        let len = items.len();
        trace!("item pool, append {} items", len);
        let mut header_items = self.reserved_items.lock();

        let to_reserve = self.lines_to_reserve - header_items.len();
        if to_reserve > 0 {
            let num_to_reserve = min(to_reserve, items.len());
            let to_append = items.split_off(num_to_reserve);
            header_items.extend(items);
            self.pool.append_vec(to_append);
        } else {
            self.pool.append_vec(items);
        }
        self.length.store(self.pool.len(), Ordering::SeqCst);
        trace!("item pool, done append {} items", len);
        self.pool.len()
    }

    pub fn take(&self) -> Vec<Chunk<Arc<dyn SkimItem>>> {
        // TODO: fix state: taken
        let ret = self.pool.snapshot();
        let num = ret.iter().map(|c| c.len()).sum();
        let taken = self.taken.swap(num, Ordering::SeqCst);
        ret
    }

    pub fn reserved(&self) -> ItemPoolGuard<Arc<dyn SkimItem>> {
        let guard = self.reserved_items.lock();
        ItemPoolGuard { guard, start: 0 }
    }
}

impl SkimItemPool for ItemPool {
    fn push(&self, item: Arc<dyn SkimItem>) {
        self.append(vec![item]); // TODO: optimize
    }
}

impl SkimItemPool for defer_drop::DeferDrop<ItemPool> {
    fn push(&self, item: Arc<dyn SkimItem>) {
        self.append(vec![item]); // TODO: optimize
    }
}


pub struct ItemPoolGuard<'a, T: Sized + 'a> {
    guard: SpinLockGuard<'a, Vec<T>>,
    start: usize,
}

impl<'mutex, T: Sized> Deref for ItemPoolGuard<'mutex, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.guard[self.start..]
    }
}

//------------------------------------------------------------------------------
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RankCriteria {
    Score,
    Begin,
    End,
    NegScore,
    NegBegin,
    NegEnd,
    Length,
    NegLength,
}

pub fn parse_criteria(text: &str) -> Option<RankCriteria> {
    match text.to_lowercase().as_ref() {
        "score" => Some(RankCriteria::Score),
        "begin" => Some(RankCriteria::Begin),
        "end" => Some(RankCriteria::End),
        "-score" => Some(RankCriteria::NegScore),
        "-begin" => Some(RankCriteria::NegBegin),
        "-end" => Some(RankCriteria::NegEnd),
        "length" => Some(RankCriteria::Length),
        "-length" => Some(RankCriteria::NegLength),
        _ => None,
    }
}

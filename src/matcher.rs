use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;

use rayon::prelude::*;

use crate::item::{ItemPool, MatchedItem};
use crate::spinlock::SpinLock;
use crate::{CaseMatching, MatchEngineFactory};
use defer_drop::DeferDrop;
use std::rc::Rc;

//==============================================================================
pub struct MatcherControl {
    stopped: Arc<AtomicBool>,
    processed: Arc<AtomicUsize>,
    matched: Arc<AtomicUsize>,
    items: Arc<SpinLock<Vec<MatchedItem>>>,
    thread_matcher: JoinHandle<()>,
}

impl MatcherControl {
    pub fn get_num_processed(&self) -> usize {
        self.processed.load(Ordering::Relaxed)
    }

    pub fn get_num_matched(&self) -> usize {
        self.matched.load(Ordering::Relaxed)
    }

    pub fn kill(self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = self.thread_matcher.join();
    }

    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    pub fn into_items(self) -> Arc<SpinLock<Vec<MatchedItem>>> {
        while !self.stopped.load(Ordering::Relaxed) {}
        self.items
    }
}

//==============================================================================
pub struct Matcher {
    engine_factory: Rc<dyn MatchEngineFactory>,
    case_matching: CaseMatching,
}

impl Matcher {
    pub fn builder(engine_factory: Rc<dyn MatchEngineFactory>) -> Self {
        Self {
            engine_factory,
            case_matching: CaseMatching::default(),
        }
    }

    pub fn case(mut self, case_matching: CaseMatching) -> Self {
        self.case_matching = case_matching;
        self
    }

    pub fn build(self) -> Self {
        self
    }

    pub fn run<C>(&self, query: &str, item_pool: Arc<DeferDrop<ItemPool>>, callback: C) -> MatcherControl
    where
        C: Fn(Arc<SpinLock<Vec<MatchedItem>>>) + Send + 'static,
    {
        let matcher_engine = self.engine_factory.create_engine_with_case(query, self.case_matching);
        debug!("engine: {}", matcher_engine);
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_clone = stopped.clone();
        let processed = Arc::new(AtomicUsize::new(0));
        let processed_clone = processed.clone();
        let matched = Arc::new(AtomicUsize::new(0));
        let matched_clone = matched.clone();
        let matched_items = Arc::new(SpinLock::new(Vec::new()));
        let matched_items_clone = matched_items.clone();

        let thread_matcher = thread::spawn(move || {
            let num_taken = item_pool.num_taken();
            let items = item_pool.take();

            // 1. use rayon for parallel
            // 2. return Err to skip iteration
            //    check https://doc.rust-lang.org/std/result/enum.Result.html#method.from_iter

            trace!("matcher start, total: {}", items.len());
            let result: Result<Vec<_>, _> = items
                .into_par_iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    processed.fetch_add(1, Ordering::Relaxed);
                    if stopped.load(Ordering::Relaxed) {
                        Some(Err("matcher killed"))
                    } else if let Some(match_result) = matcher_engine.match_item(item.clone()) {
                        matched.fetch_add(1, Ordering::Relaxed);
                        Some(Ok(MatchedItem {
                            item: item.clone(),
                            rank: match_result.rank,
                            matched_range: Some(match_result.matched_range),
                            item_idx: (num_taken + index) as u32,
                        }))
                    } else {
                        None
                    }
                })
                .collect();

            if let Ok(items) = result {
                let mut pool = matched_items.lock();
                *pool = items;
                trace!("matcher stop, total matched: {}", pool.len());
            }

            callback(matched_items.clone());
            stopped.store(true, Ordering::Relaxed);
        });

        MatcherControl {
            stopped: stopped_clone,
            matched: matched_clone,
            processed: processed_clone,
            items: matched_items_clone,
            thread_matcher,
        }
    }
}

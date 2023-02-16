use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;

use once_cell::sync::Lazy;
use rayon::prelude::*;
use rayon::ThreadPool;

use crate::item::{ItemPool, MatchedItem, MatchedItemMetadata};
use crate::spinlock::SpinLock;
use crate::{CaseMatching, MatchEngineFactory, SkimItem};
use std::rc::Rc;

static MATCHER_POOL: Lazy<ThreadPool> = Lazy::new(|| {
    rayon::ThreadPoolBuilder::new()
        .build()
        .expect("Could not initialize rayon threadpool")
});

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

    pub fn run<C>(&self, query: &str, disabled: bool, item_pool: Arc<ItemPool>, callback: C) -> MatcherControl
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

        // shortcut for when there is no query or query is disabled
        let matcher_disabled = disabled || query.is_empty();

        let thread_matcher = thread::spawn(move || {
            let num_taken = item_pool.num_taken();
            let items = item_pool.take();

            // 1. use rayon for parallel
            // 2. return Err to skip iteration
            //    check https://doc.rust-lang.org/std/result/enum.Result.html#method.from_iter

            trace!("matcher start, total: {}", items.len());

            let filter_op = |index: usize, item: &Arc<dyn SkimItem>| -> Option<Result<MatchedItem, &str>> {
                processed.fetch_add(1, Ordering::Relaxed);

                if matcher_disabled {
                    return Some(Ok(MatchedItem {
                        item: item.clone(),
                        metadata: None,
                    }));
                }

                if stopped.load(Ordering::Relaxed) {
                    return Some(Err("matcher killed"));
                }

                matcher_engine.match_item(item.as_ref()).map(|match_result| {
                    matched.fetch_add(1, Ordering::Relaxed);
                    Ok(MatchedItem {
                        item: item.clone(),
                        metadata: {
                            Some(Box::new({
                                MatchedItemMetadata {
                                    rank: match_result.rank,
                                    matched_range: Some(match_result.matched_range),
                                    item_idx: (num_taken + index) as u32,
                                }
                            }))
                        },
                    })
                })
            };

            let result: Result<Vec<_>, _> = MATCHER_POOL.install(|| {
                items
                    .par_iter()
                    .enumerate()
                    .filter_map(|(index, item)| filter_op(index, item))
                    .collect()
            });

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

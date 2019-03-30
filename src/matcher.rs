use crate::engine::EngineFactory;
pub use crate::engine::MatcherMode;
use crate::item::{ItemPool, MatchedItem};
use crate::options::SkimOptions;
use crate::spinlock::SpinLock;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;

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
        self.items.clone()
    }
}

pub struct Matcher {
    mode: MatcherMode,
}

impl Matcher {
    pub fn new() -> Self {
        Matcher {
            mode: MatcherMode::Fuzzy,
        }
    }

    pub fn with_options(options: &SkimOptions) -> Self {
        let mut matcher = Self::new();
        matcher.parse_options(&options);
        matcher
    }

    fn parse_options(&mut self, options: &SkimOptions) {
        if options.exact {
            self.mode = MatcherMode::Exact;
        }

        if options.regex {
            self.mode = MatcherMode::Regex;
        }
    }

    pub fn run<C>(
        &self,
        query: &str,
        item_pool: Arc<ItemPool>,
        mode: Option<MatcherMode>,
        callback: C,
    ) -> MatcherControl
    where
        C: Fn(Arc<SpinLock<Vec<MatchedItem>>>) + Send + 'static,
    {
        let matcher_engine = EngineFactory::build(&query, mode.unwrap_or(self.mode));
        let stopped = Arc::new(AtomicBool::new(false));
        let stopped_clone = stopped.clone();
        let processed = Arc::new(AtomicUsize::new(0));
        let processed_clone = processed.clone();
        let matched = Arc::new(AtomicUsize::new(0));
        let matched_clone = matched.clone();
        let matched_items = Arc::new(SpinLock::new(Vec::new()));
        let matched_items_clone = matched_items.clone();

        let thread_matcher = thread::spawn(move || {
            let items = item_pool.take();

            // 1. use rayon for parallel
            // 2. return Err to skip iteration
            //    check https://doc.rust-lang.org/std/result/enum.Result.html#method.from_iter

            let result: Result<Vec<_>, _> = items
                .par_iter()
                .filter_map(|item| {
                    processed.fetch_add(1, Ordering::Relaxed);
                    if stopped.load(Ordering::Relaxed) {
                        Some(Err("matcher killed"))
                    } else if let Some(item) = matcher_engine.match_item(item.clone()) {
                        matched.fetch_add(1, Ordering::Relaxed);
                        Some(Ok(item))
                    } else {
                        None
                    }
                })
                .collect();

            if let Ok(items) = result {
                let mut pool = matched_items.lock();
                *pool = items;
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

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use event::{Event, EventArg};
use item::{Item, ItemGroup, MatchedItem, MatchedItemGroup, MatchedRange};
use std::thread;

use getopts;
use score;
use std::io::Write;
use regex::Regex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

#[derive(Clone, Copy)]
enum Algorithm {
    Fuzzy,
    Regex,
    PrefixExact,
    SuffixExact,
    Exact,
    InverseExact,
    InverseSuffixExact,
}

pub struct Matcher {
    tx_result: Sender<(Event, EventArg)>,
    rank_criterion: Vec<RankCriteria>,
    is_exact: bool,
}

impl Matcher {
    pub fn new(tx_result: Sender<(Event, EventArg)>) -> Self {
        Matcher {
            tx_result: tx_result,
            rank_criterion: vec![RankCriteria::Score, RankCriteria::Index, RankCriteria::Begin, RankCriteria::End],
            is_exact: false,
        }
    }

    pub fn parse_options(&mut self, options: &getopts::Matches) {
        if let Some(tie_breaker) = options.opt_str("t") {
            let mut vec = Vec::new();
            for criteria in tie_breaker.split(',') {
                if let Some(c) = parse_criteria(criteria) {
                    vec.push(c);
                }
            }
            self.rank_criterion = vec;
        }

        if options.opt_present("exact") {
            self.is_exact = true;
        }
    }

    pub fn run(&self, rx_item: Receiver<(Event, EventArg)>) {
        let (tx_matcher, rx_matcher) = channel();
        let matcher_restart = Arc::new(AtomicBool::new(false));
        // start a new thread listening for EvMatcherRestart, that means the query had been
        // changed, so that matcher shoudl discard all previous events.
        {
            let matcher_restart = matcher_restart.clone();
            thread::spawn(move || {
                while let Ok((ev, arg)) = rx_item.recv() {
                    match ev {
                        Event::EvMatcherRestart => {
                            matcher_restart.store(true, Ordering::Relaxed);
                            while matcher_restart.load(Ordering::Relaxed) {
                                thread::sleep(Duration::from_millis(10));
                            }

                            tx_matcher.send((ev, arg));
                        }
                        _ => {
                            // pass through all other events
                            tx_matcher.send((ev, arg));
                        }
                    }
                }
            });
        }

        let mut matcher_engine: Option<MatchingEngine> = None;
        let mut num_processed: usize = 0;
        loop {

            if matcher_restart.load(Ordering::Relaxed) {
                while let Ok(_) = rx_matcher.try_recv() {}
                matcher_restart.store(false, Ordering::Relaxed);
            }

            if let Ok((ev, arg)) = rx_matcher.recv_timeout(Duration::from_millis(10)) {
                match ev {
                    Event::EvMatcherNewItem => {
                        let items: ItemGroup = *arg.downcast().unwrap();
                        num_processed += items.len();

                        matcher_engine.as_ref().map(|mat| {
                            let matched_items: MatchedItemGroup = items.into_iter()
                                .map(|item| mat.match_item(item))
                                .filter(Option::is_some)
                                .map(|item| item.unwrap())
                                .collect();
                            let _ = self.tx_result.send((Event::EvModelNewItem, Box::new(matched_items)));
                        });

                        // report the number of processed items
                        let _ = self.tx_result.send((Event::EvModelNotifyProcessed, Box::new(num_processed)));
                    }

                    Event::EvReaderStopped | Event::EvSenderWaiting => {
                        let _ = self.tx_result.send((ev, arg));
                    }
                    Event::EvSenderStopped => {
                        // Since matcher is single threaded, sender stopped means all items are
                        // processed.
                        let _ = self.tx_result.send((Event::EvModelNotifyProcessed, Box::new(num_processed)));
                        let _ = self.tx_result.send((Event::EvMatcherStopped, arg));
                    }

                    Event::EvReaderStarted => { let _ = self.tx_result.send((ev, arg)); }

                    Event::EvMatcherRestart => {
                        num_processed = 0;
                        let query = arg.downcast::<String>().unwrap();

                        // notifiy the model that the query had been changed
                        let _ = self.tx_result.send((Event::EvModelRestart, Box::new(true)));

                        matcher_engine = Some(MatchingEngine::builder(&query, self.is_exact)
                                              .rank(&self.rank_criterion)
                                              .build());
                    }

                    _ => {}
                }
            }
        }
    }

}

type ExactFilter = Box<Fn(&Item, &Option<((usize, usize), (usize, usize))>) -> bool>;

struct MatchingEngine<'a> {
    query: String,
    query_chars: Vec<char>,
    query_lower_chars: Vec<char>,
    query_regex: Option<Regex>,
    rank_criterion: Option<&'a [RankCriteria]>,
    algorithm: Algorithm
}

impl<'a> MatchingEngine<'a> {
    pub fn builder(query: &str, is_exact: bool) -> Self {
        let (algo, query) = if query.starts_with('\'') {
            if is_exact {
                (Algorithm::Fuzzy, &query[1..])
            } else {
                (Algorithm::Exact, &query[1..])
            }
        } else if query.starts_with('^') {
            (Algorithm::PrefixExact, &query[1..])
        } else if query.starts_with('!') {
            if query.ends_with('$') {
                (Algorithm::InverseSuffixExact, &query[1..(query.len()-1)])
            } else {
                (Algorithm::InverseExact, &query[1..])
            }
        } else if query.ends_with('$') {
            (Algorithm::SuffixExact, &query[..(query.len()-1)])
        } else if is_exact {
            (Algorithm::Exact, query)
        } else {
            (Algorithm::Fuzzy, query)
        };

        MatchingEngine {
            query: query.to_string(),
            query_chars: query.chars().collect(),
            query_lower_chars: query.to_lowercase().chars().collect(),
            query_regex:  Regex::new(query).ok(),
            rank_criterion: None,
            algorithm: algo,
        }
    }

    pub fn rank(mut self, rank: &'a [RankCriteria]) -> Self {
        self.rank_criterion = Some(rank);
        self
    }

    pub fn build(self) -> Self {
        self
    }

    pub fn match_item(&self, item: Arc<Item>) -> Option<MatchedItem> {
        match self.algorithm {
            Algorithm::Fuzzy => self.match_item_fuzzy(item),
            Algorithm::Regex => self.match_item_regex(item),
            Algorithm::Exact => {
                self.match_item_exact(item, Box::new(|_, matched_result| {
                    matched_result.is_some()
                }))
            }
            Algorithm::InverseExact => {
                self.match_item_exact(item, Box::new(|_, matched_result| {
                    matched_result.is_none()
                }))
            }
            Algorithm::PrefixExact => {
                self.match_item_exact(item, Box::new(|_, matched_result| {
                    match *matched_result {
                        Some(((start_pos, _), _)) => start_pos == 0,
                        None => false
                    }
                }))
            }
            Algorithm::SuffixExact => {
                self.match_item_exact(item, Box::new(|item, matched_result| {
                    match *matched_result {
                        Some((_, (_, last_pos))) => last_pos == item.get_lower_chars().len(),
                        None => false
                    }
                }))
            }
            Algorithm::InverseSuffixExact => {
                self.match_item_exact(item, Box::new(|item, matched_result| {
                    match *matched_result {
                        Some((_, (_, last_pos))) => last_pos != item.get_lower_chars().len(),
                        None => true
                    }
                }))
            }
        }
    }

    fn build_rank(&self, score: i64, index: i64, begin: i64, end: i64) -> [i64; 4] {
        self.rank_criterion.map(|criterion| {
            let mut rank = [0; 4];
            for (idx, criteria) in criterion.iter().enumerate().take(4) {
                rank[idx] = match *criteria {
                    RankCriteria::Score    => score,
                    RankCriteria::Index    => index,
                    RankCriteria::Begin    => begin,
                    RankCriteria::End      => end,
                    RankCriteria::NegScore => -score,
                    RankCriteria::NegIndex => -index,
                    RankCriteria::NegBegin => -begin,
                    RankCriteria::NegEnd   => -end,
                }
            }
            rank
        }).unwrap_or([0; 4])
    }

    fn match_item_regex(&self, item: Arc<Item>) -> Option<MatchedItem> {
        let mut matched_result = None;
        for &(start, end) in item.get_matching_ranges() {
            if self.query == "" {
                matched_result = Some((0, 0));
                break;
            }

            let source: String = item.get_lower_chars()[start .. end].iter().cloned().collect();
            matched_result = score::regex_match(&source, &self.query_regex);

            if matched_result == None {
                continue;
            }
        }

        if matched_result == None {
            return None;
        }

        let (begin, end) = matched_result.unwrap();

        let score = end - begin;
        let rank = self.build_rank(-score, item.get_index() as i64, begin, end);

        Some(MatchedItem::builder(item)
             .rank(rank)
             .matched_range(MatchedRange::Range(begin as usize, end as usize))
             .build())
    }

    fn match_item_fuzzy(&self, item: Arc<Item>) -> Option<MatchedItem> {
        // iterate over all matching fields:
        let mut matched_result = None;
        for &(start, end) in item.get_matching_ranges() {
            let source = &item.get_lower_chars()[start .. end];

            matched_result = score::fuzzy_match(source, &self.query_chars, &self.query_lower_chars);

            if matched_result == None {
                continue;
            }
        }

        if matched_result == None {
            return None;
        }

        let (score, matched_range) = matched_result.unwrap();

        let begin = *matched_range.get(0).unwrap_or(&0) as i64;
        let end = *matched_range.last().unwrap_or(&0) as i64;

        let rank = self.build_rank(-score, item.get_index() as i64, begin, end);

        Some(MatchedItem::builder(item)
             .rank(rank)
             .matched_range(MatchedRange::Chars(matched_range))
             .build())
    }

    fn match_item_exact(&self, item: Arc<Item>, filter: ExactFilter) -> Option<MatchedItem>{
        let mut matched_result = None;
        for &(start, end) in item.get_matching_ranges() {
            if self.query == "" {
                matched_result = Some(((0, 0), (0, 0)));
                break;
            }

            let chars: Vec<_> = item.get_text().chars().collect();
            let source: String = chars[start .. end].iter().cloned().collect();
            matched_result = score::exact_match(&source, &self.query);

            if matched_result == None {
                continue;
            }
        }

        if !filter(&item, &matched_result){
            return None;
        }

        let (first, _) = matched_result.unwrap_or(((0,0), (0,0)));

        let (begin, end) = first;
        let score = (end - begin) as i64;
        let rank = self.build_rank(-score, item.get_index() as i64, begin as i64, end as i64);

        Some(MatchedItem::builder(item)
             .rank(rank)
             .matched_range(MatchedRange::Range(begin, end))
             .build())
    }
}

#[derive(Debug)]
pub enum RankCriteria {
    Score,
    Index,
    Begin,
    End,
    NegScore,
    NegIndex,
    NegBegin,
    NegEnd,
}

pub fn parse_criteria(text: &str) -> Option<RankCriteria> {
    match text.to_lowercase().as_ref() {
        "score"  => Some(RankCriteria::Score),
        "index"  => Some(RankCriteria::Index),
        "begin"  => Some(RankCriteria::Begin),
        "end"    => Some(RankCriteria::End),
        "-score" => Some(RankCriteria::NegScore),
        "-index" => Some(RankCriteria::NegIndex),
        "-begin" => Some(RankCriteria::NegBegin),
        "-end"   => Some(RankCriteria::NegEnd),
        _ => None,
    }
}

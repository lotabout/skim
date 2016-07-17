/// Given a list of entries `items` and the query string, filter out the
/// matched entries using fuzzy search algorithm.


extern crate num_cpus;

use std::sync::{Arc, RwLock, Mutex};

use std::sync::mpsc::channel;
use std::collections::HashMap;
use event::Event;
use item::{Item, MatchedItem, MatchedRange};
use util::eventbox::EventBox;
use score;
use orderedvec::OrderedVec;
use std::cmp::min;
use std::thread;
use std::time::Instant;
use getopts;
use regex::Regex;

const MATCHER_CHUNK_SIZE: usize = 100;
const PROCESS_START_UPDATE_DURATION: u32 = 200; // milliseconds
const PROCESS_UPDATE_DURATION: u64 = 100; // milliseconds
const RESULT_UPDATE_DURATION: u64 = 300; // milliseconds

#[derive(Clone, Copy)]
enum Algorithm {
    FUZZY,
    REGEX,
}

pub struct Matcher {
    pub eb_req: Arc<EventBox<Event>>,       // event box that recieve requests
    eb_notify: Arc<EventBox<Event>>,    // event box that send out notification
    items: Arc<RwLock<Vec<Item>>>,
    new_items: Arc<RwLock<Vec<Item>>>,
    query: Query,
    cache: HashMap<String, MatcherCache>,
    partitions: usize,
    rank_criterion: Arc<Vec<RankCriteria>>,
    is_interactive: bool,
    algorithm: Algorithm,
}

impl<'a> Matcher {
    pub fn new(items: Arc<RwLock<Vec<Item>>>,
               new_items: Arc<RwLock<Vec<Item>>>,
               eb_notify: Arc<EventBox<Event>>) -> Self {

        let mut cache = HashMap::new();
        cache.entry("".to_string()).or_insert(MatcherCache::new());

        Matcher {
            eb_req: Arc::new(EventBox::new()),
            eb_notify: eb_notify,
            items: items,
            new_items: new_items,
            query: Query::new(""),
            cache: cache,
            partitions: num_cpus::get(),
            rank_criterion: Arc::new(vec![RankCriteria::Score,
                                          RankCriteria::Index,
                                          RankCriteria::Begin,
                                          RankCriteria::End]),
            is_interactive: false,
            algorithm: Algorithm::FUZZY,
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
            self.rank_criterion = Arc::new(vec);
        }

        if options.opt_present("i") {
            self.is_interactive = true;
        }

        if options.opt_present("regex") {
            self.algorithm = Algorithm::REGEX;
        }

        if let Some(query) = options.opt_str("q") {
            self.query = Query::new(&query);
            self.cache.entry(query.to_string()).or_insert(MatcherCache::new());
        }
    }

    pub fn process(&mut self) {
        let ref mut cache = self.cache.get_mut(&self.query.get()).unwrap();

        let query = Arc::new(self.query.clone());
        let (tx, rx) = channel();
        let mut guards = vec![];

        let start_pos = Arc::new(Mutex::new(cache.item_pos));

        for _ in 0..self.partitions {
            let items = self.items.clone();
            let start_pos = start_pos.clone();
            let query = query.clone();
            let tx = tx.clone();
            let eb_req = self.eb_req.clone();
            let criterion = self.rank_criterion.clone();
            let algorithm = self.algorithm;

            let guard = thread::spawn(move || {
                let items = items.read().unwrap();
                loop {
                    let (start, end) = { // to release the start_pos lock as soon as possible
                        let mut start_idx = start_pos.lock().unwrap();
                        if *start_idx >= items.len() {
                            break;
                        }

                        let start = *start_idx;
                        let end = min(start + MATCHER_CHUNK_SIZE, items.len());
                        *start_idx = end;
                        (start, end)
                    };

                    for i in start..end {
                        let ref item = items[i];
                        if let Some(matched) = match_item(i, &item, &query, &criterion, algorithm) {
                            let _ = tx.send(Some(matched));
                        }
                    }

                    if eb_req.peek(Event::EvMatcherResetQuery) {
                        break;
                    }
                }
                let _ = tx.send(None); // to notify match process end
            });
            guards.push(guard);
        };

        let items_len = self.items.read().unwrap().len();
        let mut matched_items = cache.matched_items.write().unwrap();
        let timer = Instant::now();
        while let Ok(result) = rx.recv() {
            if let Some(matched) = result {
                matched_items.push(matched);
            }

            let start_idx = {*start_pos.lock().unwrap()};

            // update process
            let time = timer.elapsed();
            let mills = (time.as_secs()*1000) as u32 + time.subsec_nanos()/1000/1000;
            if mills > PROCESS_START_UPDATE_DURATION {
                self.eb_notify.set_throttle(Event::EvMatcherUpdateProcess, Box::new(((start_idx+1) *100/(items_len+1)) as u64), PROCESS_UPDATE_DURATION);
            }

            if start_idx >= items_len {
                break;
            }

            if self.eb_req.peek(Event::EvMatcherResetQuery) {
                break;
            }
        }

        // wait for all threads to exit
        for guard in guards {
            let _ = guard.join();
        }

        // consume remaining results.
        while let Ok(result) = rx.try_recv() {
            if let Some(matched) = result {
                matched_items.push(matched);
            }
        }
        cache.item_pos = *start_pos.lock().unwrap();
        self.eb_notify.set(Event::EvMatcherUpdateProcess, Box::new(100 as u64));
    }

    pub fn process_interactive(&mut self) {
        let ref mut cache = self.cache.get_mut(&self.query.get()).unwrap();
        let mut matched_items = cache.matched_items.write().unwrap();
        let items_len = self.items.read().unwrap().len();
        let timer = Instant::now();
        let start_pos = cache.item_pos;
        let mut last_pos = start_pos;

        for index in start_pos..items_len {
            let mut matched = MatchedItem::new(index);
            matched.set_matched_range(MatchedRange::Range(0, 0));
            matched.set_rank(build_rank(&self.rank_criterion, index as i64, index as i64, 0, 0));

            matched_items.push(matched);

            if self.eb_req.peek(Event::EvMatcherResetQuery) {
                break;
            }

            last_pos = index+1;

            // update process
            let time = timer.elapsed();
            let mills = (time.as_secs()*1000) as u32 + time.subsec_nanos()/1000/1000;
            if mills > PROCESS_START_UPDATE_DURATION {
                self.eb_notify.set_throttle(Event::EvMatcherUpdateProcess, Box::new(((index+1) *100/(items_len+1)) as u64), PROCESS_UPDATE_DURATION);
            }
        }
        cache.item_pos = last_pos;
        self.eb_notify.set(Event::EvMatcherUpdateProcess, Box::new(100 as u64));
    }

    fn reset_query(&mut self, query: &str) {
        self.query = Query::new(query);
        if self.is_interactive {
            self.new_items.write().unwrap().clear();
            self.cache.remove(&query.to_string());
        }
        self.cache.entry(query.to_string()).or_insert(MatcherCache::new());
    }

    pub fn run(&mut self) {
        loop {
            for (e, val) in self.eb_req.wait() {
                match e {
                    Event::EvMatcherNewItem => {}
                    Event::EvMatcherResetQuery => {
                        self.reset_query(&val.downcast::<String>().unwrap());
                        if self.is_interactive {
                            self.eb_notify.set(Event::EvMatcherSync, Box::new(true));
                            let _ = self.eb_req.wait_for(Event::EvModelAck);
                        }
                    }
                    _ => {}
                }
            }

            // insert new items
            {
                let mut buffer = self.new_items.write().unwrap();
                if buffer.len() > 0 {
                    let mut items = self.items.write().unwrap();
                    items.append(&mut buffer);
                }
            }

            if self.is_interactive {
                self.process_interactive();
            } else {
                self.process();
            }

            if !self.eb_req.peek(Event::EvMatcherResetQuery) {
                let matched_items = self.cache.get_mut(&self.query.get()).unwrap().matched_items.clone();
                if self.is_interactive {
                    self.eb_notify.set_debounce(Event::EvMatcherEnd, Box::new(matched_items), RESULT_UPDATE_DURATION);
                } else {
                    self.eb_notify.set(Event::EvMatcherEnd, Box::new(matched_items));
                }
            }
        }
    }
}

fn match_item(index: usize, item: &Item, query: &Query, criterion: &[RankCriteria],
              algorithm: Algorithm) -> Option<MatchedItem> {
    match algorithm {
        Algorithm::FUZZY => match_item_fuzzy(index, item, query, criterion),
        Algorithm::REGEX => match_item_regex(index, item, query, criterion),
    }
}

fn match_item_fuzzy(index: usize, item: &Item, query: &Query, criterion: &[RankCriteria]) -> Option<MatchedItem> {
    let matched_result = score::fuzzy_match(item.get_lower_chars(), query.get_chars(), query.get_lower_chars());

    if matched_result == None {
        return None;
    }

    let (score, matched_range) = matched_result.unwrap();

    let begin = *matched_range.get(0).unwrap_or(&0) as i64;
    let end = *matched_range.last().unwrap_or(&0) as i64;

    if !query.empty() && !item.in_matching_range(begin as usize, (end+1) as usize) {
        return None;
    }

    let rank = build_rank(criterion, -score, index as i64, begin, end);

    let mut item = MatchedItem::new(index);
    item.set_matched_range(MatchedRange::Chars(matched_range));
    item.set_rank(rank);
    Some(item)
}

fn match_item_regex(index: usize, item: &Item, query: &Query, criterion: &[RankCriteria]) -> Option<MatchedItem> {
    let matched_result = if query.empty() {
        Some((0, 0))
    } else {
        score::regex_match(item.get_text(), query.get_regex())
    };

    if matched_result == None {
        return None;
    }

    let (begin, end) = matched_result.unwrap();

    if !query.empty() && !item.in_matching_range(begin, end) {
        return None;
    }

    let score = end - begin;
    let rank = build_rank(criterion, score as i64, index as i64, begin as i64, end as i64);

    let mut item = MatchedItem::new(index);
    item.set_matched_range(MatchedRange::Range(begin, end));
    item.set_rank(rank);
    Some(item)
}

fn build_rank(criterion: &[RankCriteria], score: i64, index: i64, begin: i64, end: i64) -> [i64; 4] {
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
}

struct MatcherCache {
    pub matched_items: Arc<RwLock<OrderedVec<MatchedItem>>>,
    pub item_pos: usize,
}

impl MatcherCache {
    pub fn new() -> Self {
        MatcherCache {
            item_pos: 0,
            matched_items: Arc::new(RwLock::new(OrderedVec::new())),
        }
    }
}

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

// cache for lowercases and others.
#[derive(Clone)]
struct Query {
    query: String,
    query_chars: Vec<char>,
    query_lower_chars: Vec<char>,
    regex: Option<Regex>,
}

impl Query {
    pub fn new(query: &str) -> Self {
        Query {
            query: query.to_string(),
            query_chars: query.chars().collect(),
            query_lower_chars: query.to_lowercase().chars().collect(),
            regex: Regex::new(query).ok(),
        }
    }

    pub fn get(&self) -> String {
        self.query.clone()
    }

    pub fn get_chars(&self) -> &[char] {
        &self.query_chars
    }

    pub fn get_lower_chars(&self) -> &[char] {
        &self.query_lower_chars
    }

    pub fn get_regex(&self) -> &Option<Regex> {
        &self.regex
    }

    pub fn empty(&self) -> bool {
        &self.query == ""
    }
}

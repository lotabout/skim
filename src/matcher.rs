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

const MATCHER_CHUNK_SIZE: usize = 100;
const PROCESS_START_UPDATE_DURATION: u32 = 200; // milliseconds
const PROCESS_UPDATE_DURATION: u64 = 100; // milliseconds
const RESULT_UPDATE_DURATION: u64 = 300; // milliseconds

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
            let is_interactive = self.is_interactive;

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
                        if let Some(matched) = match_item(i, &item, &query, &criterion, is_interactive) {
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

            self.process();
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

fn match_item(index: usize, item: &Item, query: &Query, criterion: &[RankCriteria], is_interactive: bool) -> Option<MatchedItem> {
    let matched_result = if !is_interactive {
        score::fuzzy_match(item.get_lower_chars(), query.get_chars(), query.get_lower_chars())
    } else {
        Some((-(index as i64), Vec::new()))
    };

    if matched_result == None {
        return None;
    }

    let (score, matched_range) = matched_result.unwrap();

    let mut rank = [0; 4];
    for (idx, criteria) in criterion.iter().enumerate().take(4) {
        rank[idx] = match *criteria {
            RankCriteria::Score    => -score,
            RankCriteria::Index    => index as i64,
            RankCriteria::Begin    => *matched_range.get(0).unwrap_or(&0) as i64,
            RankCriteria::End      => *matched_range.last().unwrap_or(&0) as i64,
            RankCriteria::NegScore => score,
            RankCriteria::NegIndex => -(index as i64),
            RankCriteria::NegBegin => -(*matched_range.get(0).unwrap_or(&0) as i64),
            RankCriteria::NegEnd   => -(*matched_range.last().unwrap_or(&0) as i64),
        }
    }

    let mut item = MatchedItem::new(index);
    item.set_matched_range(MatchedRange::Chars(matched_range));
    item.set_rank(rank);
    Some(item)
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
}

impl Query {
    pub fn new(query: &str) -> Self {
        Query {
            query: query.to_string(),
            query_chars: query.chars().collect(),
            query_lower_chars: query.to_lowercase().chars().collect(),
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
}

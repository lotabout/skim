/// Given a list of entries `items` and the query string, filter out the
/// matched entries using fuzzy search algorithm.


extern crate num_cpus;
extern crate crossbeam;

use std::sync::{Arc, RwLock};

use std::sync::mpsc::channel;
use std::collections::HashMap;
use event::Event;
use item::{Item, MatchedItem, MatchedRange};
use util::eventbox::EventBox;
use score;
use orderedvec::OrderedVec;
use std::cmp::min;
use std::time::{Instant};

pub struct Matcher {
    pub eb_req: Arc<EventBox<Event>>,       // event box that recieve requests
    eb_notify: Arc<EventBox<Event>>,    // event box that send out notification
    items: Arc<RwLock<Vec<Item>>>,
    new_items: Arc<RwLock<Vec<Item>>>,
    query: String,
    cache: HashMap<String, MatcherCache>,
    partitions: usize,
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
            query: String::new(),
            cache: cache,
            partitions: num_cpus::get(),
        }
    }

    pub fn process(&mut self) {
        let ref mut cache = self.cache.get_mut(&self.query).unwrap();
        let timer = Instant::now();

        let item_pos = cache.item_pos;
        let items = self.items.read().unwrap();
        let slices = slice_items(&items, item_pos, self.partitions);
        let mut guards = vec![];
        let query = self.query.clone();
        let (tx, rx) = channel();
        let eb_req = self.eb_req.clone();
        let eb_notify = self.eb_notify.clone();

        crossbeam::scope(|scope| {
            for (start, slice) in slices {
                let query = query.clone();
                let tx = tx.clone();
                let guard = scope.spawn(move || {
                    let mut ret = vec![];
                    let mut last = start;
                    for (item, index) in slice.iter().zip(start..) {
                        if let Some(matched) = match_item(index, &item.text, &query) {
                            ret.push(matched.clone());
                        }

                        if index > last + 1000 {
                            tx.send(index - last);
                            last = index;
                        }
                    }
                    tx.send(slice.len() + start - last);
                    ret
                });
                guards.push(guard);
            }

            let mut processed_num = 0;
            while let Ok(num) = rx.recv() {
                processed_num += num;

                if processed_num >= items.len() - item_pos {
                    break;
                }

                if eb_req.peek(Event::EvMatcherResetQuery) {
                    // wait for process to exit and clean
                    while let Ok(num) = rx.try_recv() {} 
                }

                // update process
                let time = timer.elapsed();
                let mills = (time.as_secs()*1000) as u32 + time.subsec_nanos()/1000/1000;
                if mills > 200 {
                    eb_notify.set(Event::EvMatcherUpdateProcess, Box::new((processed_num*100/items.len()) as u64));
                }
            }
        });


        if !eb_req.peek(Event::EvMatcherResetQuery) {

            cache.matched_items.clear();
            for guard in guards {
                let mut matched_items = guard.join();
                while let Some(item) = matched_items.pop() {
                    cache.matched_items.push(item);
                }
            }
            cache.item_pos = items.len();
        }
    }

    fn reset_query(&mut self, query: &str) {
        self.query.clear();
        self.query.push_str(query);
        self.cache.entry(query.to_string()).or_insert(MatcherCache::new());
    }

    pub fn run(&mut self) {
        loop {
            for (e, val) in self.eb_req.wait() {
                match e {
                    Event::EvMatcherNewItem => {}
                    Event::EvMatcherResetQuery => {
                        self.reset_query(&val.downcast::<String>().unwrap());
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
            let ref mut cache = self.cache.get_mut(&self.query).unwrap();
            self.eb_notify.set(Event::EvMatcherEnd, Box::new(cache.matched_items.clone()));
        }
    }
}

fn slice_items<'a>(items: &'a[Item], start_pos: usize, partitions: usize) -> Vec<(usize, &'a[Item])>{
    let step = (items.len() - start_pos)/partitions + 1;
    let mut ret = Vec::new();
    let mut start = start_pos;
    let mut end;
    while start < items.len() {
        end = min(start + step, items.len());
        ret.push((start, &items[start..end]));
        start = end;
    }
    ret
}


fn match_item(index: usize, item: &str, query: &str) -> Option<MatchedItem> {
    let matched_result = score::fuzzy_match(item, query);
    if matched_result == None {
        return None;
    }

    let (score, matched_range) = matched_result.unwrap();

    let mut item = MatchedItem::new(index);
    item.set_matched_range(MatchedRange::Chars(matched_range));
    item.set_score(score);
    Some(item)
}


struct MatcherCache {
    pub matched_items: OrderedVec<MatchedItem>,
    pub item_pos: usize,
}

impl MatcherCache {
    pub fn new() -> Self {
        MatcherCache {
            item_pos: 0,
            matched_items: OrderedVec::new(),
        }
    }
}

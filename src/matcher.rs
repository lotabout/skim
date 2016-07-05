/// Given a list of entries `items` and the query string, filter out the
/// matched entries using fuzzy search algorithm.

use std::sync::{Arc, RwLock};

use std::sync::mpsc::Sender;
use std::collections::HashMap;
use event::Event;
use item::{Item, MatchedItem, MatchedRange};
use util::eventbox::EventBox;
use score;
use orderedvec::OrderedVec;

pub struct Matcher {
    pub eb_req: Arc<EventBox<Event>>,       // event box that recieve requests
    eb_notify: Arc<EventBox<Event>>,    // event box that send out notification
    items: Arc<RwLock<Vec<Item>>>,
    new_items: Arc<RwLock<Vec<Item>>>,
    item_pos: usize,
    num_matched: u64,
    query: String,
    cache: HashMap<String, MatcherCache>,
}

impl Matcher {
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
            item_pos: 0,
            num_matched: 0,
            query: String::new(),
            cache: cache,
        }
    }

    pub fn process(&mut self) {
        let ref mut cache = self.cache.get_mut(&self.query).unwrap();

        self.item_pos = cache.item_pos;

        loop {
            let items = self.items.read().unwrap();
            if let Some(item) = items.get(self.item_pos) {
                if let Some(matched) = match_item(self.item_pos, &item.text, &self.query) {
                    self.num_matched += 1;
                    cache.matched_items.push(matched.clone());
                }
            } else {
                break;
            }

            self.item_pos += 1;
            cache.item_pos = self.item_pos;
            (*self.eb_notify).set(Event::EvMatcherUpdateProcess, Box::new((self.num_matched, items.len() as u64, self.item_pos as u64)));

            // check if the current process need to be stopped
            if self.eb_req.peek(Event::EvMatcherResetQuery) {
                break;
            }
        }
    }

    fn reset_query(&mut self, query: &str) {
        self.query.clear();
        self.query.push_str(query);
        self.num_matched = 0;
        self.item_pos = 0;
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

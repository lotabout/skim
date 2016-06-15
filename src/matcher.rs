/// Given a list of entries `items` and the query string, filter out the
/// matched entries using fuzzy search algorithm.

use std::sync::{Arc, RwLock};

use std::sync::mpsc::Sender;
use event::Event;
use item::{Item, MatchedItem};
use util::eventbox::EventBox;

pub struct Matcher {
    tx_output: Sender<MatchedItem>,   // channel to send output to
    eb_req: Arc<EventBox<Event>>,       // event box that recieve requests
    eb_notify: Arc<EventBox<Event>>,    // event box that send out notification
    items: Arc<RwLock<Vec<Item>>>,
    item_pos: usize,
    num_matched: u64,
    query: String,
}


impl Matcher {
    pub fn new(items: Arc<RwLock<Vec<Item>>>, tx_output: Sender<MatchedItem>,
               eb_req: Arc<EventBox<Event>>, eb_notify: Arc<EventBox<Event>>) -> Self {
        Matcher {
            tx_output: tx_output,
            eb_req: eb_req,
            eb_notify: eb_notify,
            items: items,
            item_pos: 0,
            num_matched: 0,
            query: String::new(),
        }
    }

    fn match_str(&self, item: &str) -> bool {
        if self.query == "" {
            return true;
        }

        item.starts_with(&self.query)
    }

    pub fn process(&mut self) {
        let items = self.items.read().unwrap();
        for item in items[self.item_pos..].into_iter() {
            // process the matcher
            //self.tx_output.send(string.clone());
            if self.match_str(&item.text) {
                self.num_matched += 1;
                let _ = self.tx_output.send(MatchedItem::new(self.item_pos));
            }

            self.item_pos += 1;
            if (self.item_pos % 100) == 99 && !self.eb_req.is_empty() {
                break;
            }
        }
        (*self.eb_notify).set(Event::EvMatcherUpdateProcess, Box::new((self.num_matched, items.len() as u64)));
    }

    fn reset_query(&mut self, query: &str) {
        self.query.clear();
        self.query.push_str(query);
        self.num_matched = 0;
        self.item_pos = 0;
    }

    pub fn run(&mut self) {
        loop {
            for (e, val) in (*self.eb_req).wait() {
                match e {
                    Event::EvMatcherNewItem => {}
                    Event::EvMatcherResetQuery => {
                        self.reset_query(&val.downcast::<String>().unwrap());
                    }
                    _ => {}
                }
            }

            self.process()
        }
    }
}


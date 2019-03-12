use crate::reader::{Reader, ReaderControl};
use crate::matcher::{Matcher, MatcherControl};
use std::sync::Arc;
use crate::spinlock::SpinLock;
use crate::item::{Item, MatchedItem};
use crate::selection::Selection;
use skiplist::OrderedSkipList;
use std::thread;

pub struct CoordinatorControl {


}

impl CoordinatorControl {
    pub fn kill(self) {
    }
}

pub struct Coordinator {
    reader: Reader,
    reader_control: Option<ReaderControl>,
    matcher: Matcher,
    matcher_control: Option<MatcherControl>,
    coordinator_control: Option<CoordinatorControl>,
    item_pool: Arc<SpinLock<Vec<Arc<Item>>>>,
    matched_items: Arc<SpinLock<OrderedSkipList<Arc<MatchedItem>>>>,
    last_cmd: String,
    last_query: String,
}

impl Coordinator {

    pub fn run(&mut self, cmd: &str, query: &str) {
        if cmd != self.last_cmd {
            self.reader_control.take().map(|c|c.kill());
            self.coordinator_control.take().map(|c|c.kill());
            let mut matched = self.matched_items.lock();
            matched.clear();

            // start reader
            self.reader_control.replace(self.reader.run(cmd));
            self.matcher.run(query, )

            self.matcher_control.replace(self.matcher.run(&query, items_to_match, None, move |_| {
                let _ = tx_clone.send((Event::EvMatcherDone, Box::new(true)));
            }));




            // stop the world and restart
        } else if query != self.last_query {
            // stop reader and restart

        } else {
            // do nothing

        }
    }


}

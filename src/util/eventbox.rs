/// eventbox is a simple abstract of event handling
///
/// The concept is:
///
/// An eventbox stores a vector of events, the order does not mather.
///
/// 1. the sender and receiver of a/some events share an eventbox
/// 2. when some event happans, the sender push the event into the eventbox.
/// 3. Meanwhile the receiver is waiting(blocked).
/// 4. When some event happen, eventbox will notify the receiver.

use std::sync::{Condvar, Mutex};
use std::collections::HashMap;
use std::any::Any;

pub type EventType = i32;

pub type Value = Box<Any + 'static + Send>;
pub type Events = HashMap<EventType, Value>;

pub struct EventBox {
    mutex: Mutex<bool>, // value doesn't matter.
    events: Events,
    cond: Condvar,
    ignore: Events,
}

impl EventBox {
    pub fn new() -> Self {
        EventBox {
            events: HashMap::new(),
            mutex: Mutex::new(false),
            cond: Condvar::new(),
            ignore: HashMap::new(),
        }
    }

    /// wait: wait for an event(any) to fire
    /// if any event is triggered, run callback on events vector
    pub fn wait(&mut self, mut callback: Box<FnMut(&mut Events)>) {
        let mtx = self.mutex.lock().unwrap();
        let num_of_events = self.events.len();
        if num_of_events == 0 {
            self.cond.wait(mtx);
        }
        callback(&mut self.events);
    }

    /// set: fires an event
    pub fn set(&mut self, e: EventType, value: Value) {
        self.mutex.lock();
        let val = self.events.entry(e).or_insert(Box::new(0));
        *val = value;
        if !self.ignore.contains_key(&e) {
            self.cond.notify_all();
        }
    }

    /// clear the event map
    pub fn clear(&mut self) {
        self.events.clear();
    }

    // peek at the event box to check whether event had been set or not
    pub fn peek(&self, event: EventType) -> bool {
        self.mutex.lock();
        self.events.contains_key(&event)
    }

    // remove events from ignore table
    pub fn watch(&mut self, events: &Vec<EventType>) {
        for e in events {
            self.ignore.remove(e);
        }
    }

    // add events from ignore table
    pub fn unwatch(&mut self, events: &Vec<EventType>) {
        for e in events {
            self.ignore.insert(*e, Box::new(true));
        }
    }

    pub fn wait_for(&mut self, event: EventType) {
        let mut event_found = false;
        while !event_found {
            self.wait(Box::new(move |events| {
                let target = event;
                for (e, val) in events {
                    match *e {
                       target => {
                           event_found = true;
                           return;
                       }
                    }
                }
            }));
        }
    }
}

unsafe impl Sync for EventBox {}


#[cfg(test)]
mod test {
    use super::*;
    use std::thread;
    use std::sync::Arc;
    use std::mem;

    #[test]
    fn test_eventbox() {
        let mut eb = Arc::new(EventBox::new());

        //let mut borrow_1 = eb.clone();
        //let mut borrow_2 = eb.clone();

        println!("Start waiting--------------------------");
        let mut eventbox = Arc::get_mut(&mut eb).unwrap();
        let mut ret = 0;
        eventbox.wait(Box::new(move |events| {
            for (e, val) in events {
                match *e {
                    10 => {
                        let x: i32 = *mem::replace(val, Box::new(0)).downcast::<i32>().unwrap();
                        println!("ret = {}", x);
                        return;
                    }
                    _ => {return;}
                }
            }
        }));
        println!("Stop waiting--------------------------");

        //thread::spawn(move || {
            //// calculate some value
            //Arc::get_mut(&mut borrow_1).map(|e| e.set(10, Box::new(10)));
        //});
        assert_eq!(10, 20);
    }
}

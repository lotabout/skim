/// eventbox is a simple abstract of event handling
///
/// The concept is:
///
/// An eventbox stores a vector of events, the order does not mather.
///
/// 1. the sender and receiver of a/some events share an eventbox
/// 2. when some event happans, the sender push the event/value into the eventbox.
/// 3. Meanwhile the receiver is waiting(blocked).
/// 4. When some event happen, eventbox will notify the receiver.
///
/// # Examples
/// ```
/// use std::sync::Arc;
/// use std::thread;
/// use fzf_rs::util::eventbox::EventBox;
/// let mut eb = Arc::new(EventBox::new());
/// let mut eb2 = eb.clone();
///
/// thread::spawn(move || {
///     eb2.set(10, Box::new(20));
/// });
///
/// let val: i32 = *eb.wait_for(10).downcast().unwrap();
/// assert_eq!(20, val);
/// ```

use std::sync::{Condvar, Mutex};
use std::collections::HashMap;
use std::any::Any;
use std::mem;
use std::hash::Hash;

pub type Value = Box<Any + 'static + Send>;
pub type Events<T> = HashMap<T, Value>;

struct EventData<T> {
    events: Events<T>,
    ignore: Events<T>,
}

pub struct EventBox<T> {
    mutex: Mutex<EventData<T>>,
    cond: Condvar,
}

impl<T: Hash + Eq + Copy> EventBox<T> {
    pub fn new() -> Self {
        EventBox {
            mutex: Mutex::new(EventData{events: HashMap::new(), ignore: HashMap::new()}),
            cond: Condvar::new(),
        }
    }

    /// wait: wait for an event(any) to fire
    /// if any event is triggered, run callback on events vector
    pub fn wait(&self) -> Events<T> {
        let mut data = self.mutex.lock().unwrap();
        let events = mem::replace(&mut data.events, HashMap::new());
        let num_of_events = events.len();
        if num_of_events == 0 {
            let _ = self.cond.wait(data);
        }
        events
    }

    /// set: fires an event
    pub fn set(&self, e: T, value: Value) {
        let mut data = self.mutex.lock().unwrap();
        {
            let val = data.events.entry(e).or_insert(Box::new(0));
            *val = value;
        }
        if !data.ignore.contains_key(&e) {
            self.cond.notify_all();
        }
    }

    /// clear the event map
    pub fn clear(&self) {
        let mut data = self.mutex.lock().unwrap();
        data.events.clear();
    }

    // peek at the event box to check whether event had been set or not
    pub fn peek(&self, event: T) -> bool {
        let data = self.mutex.lock().unwrap();
        data.events.contains_key(&event)
    }

    // remove events from ignore table
    pub fn watch(&self, events: &Vec<T>) {
        let mut data = self.mutex.lock().unwrap();
        for e in events {
            data.ignore.remove(e);
        }
    }

    // add events from ignore table
    pub fn unwatch(&self, events: &Vec<T>) {
        let mut data = self.mutex.lock().unwrap();
        for e in events {
            data.ignore.insert(*e, Box::new(true));
        }
    }

    pub fn wait_for(&self, event: T) -> Value {
        'event_found: loop {
            for (e, val) in self.wait() {
                if e == event {
                    return val
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        let data = self.mutex.lock().unwrap();
        data.events.len() == 0
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::thread;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn test_wait() {
        const NUM_OF_EVENTS: i32 = 4;

        // create `NUM_OF_EVENTS` threads that set the return value to
        // their thread number, the sum up the value and compare it in
        // the main thread.

        let eb = Arc::new(EventBox::new());
        let counter = Arc::new(Mutex::new(0));
        for i in 1..(NUM_OF_EVENTS+1) {
            let eb_clone = eb.clone();
            let counter_clone = counter.clone();
            thread::spawn(move || {
                eb_clone.set(i, Box::new(i));
                let mut count = counter_clone.lock().unwrap();
                *count += 1;
            });
        }

        // wait till all events are set
        loop {
            thread::sleep(Duration::from_millis(100));
            let count = counter.lock().unwrap();
            if *count == NUM_OF_EVENTS {
                break;
            }
        }


        let mut total: i32 = 0;
        for (_, val) in eb.wait() {
            total += *val.downcast().unwrap();
        }

        assert_eq!((1..(NUM_OF_EVENTS+1)).fold(0, |x, acc| acc+x), total);
    }

    #[test]
    fn test_wait_for() {
        let eb = Arc::new(EventBox::new());
        let eb2 = eb.clone();

        thread::spawn(move || {
            eb2.set(10, Box::new(20));
        });

        let val: i32 = *eb.wait_for(10).downcast().unwrap();
        assert_eq!(20, val);
    }

    #[test]
    fn test_unwatch_set() {
        const NUM_OF_EVENTS: i32 = 4;
        let eb = Arc::new(EventBox::new());

        // this time, we ignore event NO. NUM_OF_EVENTS
        // note that unwatch will not trigger the event notification, but the
        // value are actually set.
        eb.unwatch(&vec![NUM_OF_EVENTS]);

        let counter = Arc::new(Mutex::new(0));
        for i in 1..(NUM_OF_EVENTS+1) {
            let eb_clone = eb.clone();
            let counter_clone = counter.clone();
            thread::spawn(move || {
                eb_clone.set(i, Box::new(i));
                let mut count = counter_clone.lock().unwrap();
                *count += 1;
            });
        }

        // wait till all events are set
        loop {
            thread::sleep(Duration::from_millis(100));
            let count = counter.lock().unwrap();
            if *count == NUM_OF_EVENTS {
                break;
            }
        }

        let mut total: i32 = 0;
        for (_, val) in eb.wait() {
            total += *val.downcast().unwrap();
        }

        assert_eq!((1..(NUM_OF_EVENTS+1)).fold(0, |x, acc| acc+x), total);
    }

    #[test]
    fn test_unwatch_notify() {
        // unwatched event will not trigger notification

        let eb = Arc::new(EventBox::new());

        eb.unwatch(&vec![1]);

        let eb_clone1 = eb.clone();

        let which_one = Arc::new(Mutex::new(1));
        let which_one_clone = which_one.clone();

        let wait = thread::spawn(move || {
            for (_, _) in eb.wait() {
                let mut data = which_one.lock().unwrap();
                assert_eq!(2, *data);
            }
        });

        thread::spawn(move || {
            eb_clone1.set(1, Box::new(1));

            // to ensure that the `wait` in main thread have time to trigger.
            // of course, the expected behavior is that it will not.
            thread::sleep(Duration::from_millis(100));

            let mut data = which_one_clone.lock().unwrap();
            *data = 2;
            eb_clone1.set(2, Box::new(2));
        });

        wait.join();
    }
}

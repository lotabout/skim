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
/// use skim::util::eventbox::EventBox;
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

use std::sync::{Condvar, Mutex, Arc};
use std::collections::{HashMap, HashSet};
use std::any::Any;
use std::mem;
use std::hash::Hash;
use std::thread;
use std::time::Duration;

pub type Value = Box<Any + 'static + Send>;
pub type Events<T> = HashMap<T, Value>;

struct EventData<T> {
    events:    Events<T>,
    lazy:      HashSet<T>,
    blocked:   HashSet<T>,
    throttled: Events<T>,
}

pub struct EventBox<T> {
    mutex: Arc<Mutex<EventData<T>>>,
    cond: Arc<Condvar>,
}

impl<T> EventBox<T> where T: Hash + Eq + Copy + 'static + Send {
    pub fn new() -> Self {
        EventBox {
            mutex: Arc::new(Mutex::new(EventData{events:    HashMap::new(),
                                                 lazy:      HashSet::new(),
                                                 throttled: HashMap::new(),
                                                 blocked:   HashSet::new()})),
            cond: Arc::new(Condvar::new()),
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
            let mut data = self.mutex.lock().unwrap();
            mem::replace(&mut data.events, HashMap::new())
        } else {
            events
        }
    }

    /// set: fires an event
    pub fn set(&self, e: T, value: Value) {
        set_event(&self.mutex, &self.cond, e, value)
    }

    /// set_throttle: limit the number of an event within a timeout.
    ///  X X XX XY Y YY Y
    /// |        |        |
    ///  X        X        Y
    pub fn set_throttle(&self, e: T, value: Value, timeout: u64) {
        set_event_throttle(&self.mutex, &self.cond, e, value, timeout, false);
    }

    pub fn set_debounce(&self, e: T, value: Value, timeout: u64) {
        set_event_debounce(&self.mutex, &self.cond, e, value, timeout);
    }

    // peek at the event box to check whether event had been set or not
    pub fn peek(&self, event: T) -> bool {
        let data = self.mutex.lock().unwrap();
        data.events.contains_key(&event)
    }

    pub fn wait_for(&self, event: T) -> Value {
        loop {
            let mut data = self.mutex.lock().unwrap();
            let value = data.events.remove(&event);
            if value.is_some() {
                return value.unwrap();
            }
            let _ = self.cond.wait(data);
        }
    }

    pub fn clear(&self) {
        let mut data = self.mutex.lock().unwrap();
        data.events.clear();
        data.lazy.clear();
        data.blocked.clear();
        data.throttled.clear();
    }
}


fn set_event<T>(mutex: &Arc<Mutex<EventData<T>>>, cond: &Arc<Condvar>, e: T, value: Value)
    where T: Hash + Eq + Copy + 'static + Send {
    let mut data = mutex.lock().unwrap();
    {

        let val = data.events.entry(e).or_insert(Box::new(0));
        *val = value;
    }

    if !data.lazy.contains(&e) {
        cond.notify_all();
    }
}

fn set_event_throttle<T>(mutex: &Arc<Mutex<EventData<T>>>, cond: &Arc<Condvar>, e: T, value: Value, timeout: u64, from_thread: bool)
    where T: Hash + Eq + Copy + 'static + Send {
    {
        let mut data = mutex.lock().unwrap();
        if !from_thread && data.blocked.contains(&e) {
            let val = data.throttled.entry(e).or_insert(Box::new(0));
            *val = value;
            return;
        } else {
            data.blocked.insert(e);
        }
    }

    set_event(mutex, cond, e, value);

    let mutex = mutex.clone();
    let cond = cond.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(timeout));
        let remaining = {
            let mut data = mutex.lock().unwrap();
            data.throttled.remove(&e)
        };

        remaining.map_or_else(
            || {
                let mut data = mutex.lock().unwrap();
                let _ = data.blocked.remove(&e);
            },
            |v| {
                set_event_throttle(&mutex, &cond, e, v, timeout, true);
            });
    });
}

fn set_event_debounce<T>(mutex: &Arc<Mutex<EventData<T>>>, cond: &Arc<Condvar>, e: T, value: Value, timeout: u64)
    where T: Hash + Eq + Copy + 'static + Send {
    {
        let mut data = mutex.lock().unwrap();
        let val = data.throttled.entry(e).or_insert(Box::new(0));
        *val = value;
    }
    {
        let mut data = mutex.lock().unwrap();
        if data.blocked.contains(&e) {
            return;
        } else {
            data.blocked.insert(e);
        }
    }

    let mutex = mutex.clone();
    let cond = cond.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(timeout));
        let remaining = {
            let mut data = mutex.lock().unwrap();
            data.throttled.remove(&e)
        };

        remaining.map(|v| { set_event(&mutex, &cond, e, v); });
        let mut data = mutex.lock().unwrap();
        let _ = data.blocked.remove(&e);
    });
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
            eb2.set(20, Box::new(20));
            eb2.set(10, Box::new(20));
        });

        let val: i32 = *eb.wait_for(10).downcast().unwrap();
        assert_eq!(20, val);
        assert!(eb.peek(20));
    }

    // Not including this test, because we should not rely on eventbox to do the dispatching at
    // exact time. In this sense, it is better not to include this in the test suite.
    //
    //#[test]
    //fn test_set_throttle() {
        //let eb = Arc::new(EventBox::new());

        //let eb_clone = eb.clone();
        //thread::spawn(move || {
            //// will receive: 0, 2, 5, 7
            //for i in 0..10 {
                //eb_clone.set_throttle(1, Box::new(i), 20);
                //thread::sleep(Duration::from_millis(7));
            //}
        //});

        //let mut total: i32 = 0;
        //let timer = Instant::now();
        //loop {

            //if eb.peek(1) {
                //for (_, val) in eb.wait() {
                    //let x = *val.downcast().unwrap();
                    //println!("x = {}", x);
                    //total += x;
                //}
            //}

            //let time = timer.elapsed();
            //let mills = (time.as_secs()*1000) as u32 + time.subsec_nanos()/1000/1000;
            //if mills > 100 {
                //break;
            //}
        //}

        //assert_eq!(total, 24);
    //}

}

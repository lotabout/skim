use std::sync::mpsc::{Receiver, SyncSender};
use item::Item;
use event::{Event, EventArg};
use std::thread;
use std::time::Duration;

use std::io::Write;
macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

// sender is a cache of reader
pub struct CachedSender {
    items: Vec<Item>, // cache
    rx_sender: Receiver<(Event, EventArg)>,
    tx_item: SyncSender<(Event, EventArg)>,
}

impl CachedSender {
    pub fn new(rx_sender: Receiver<(Event, EventArg)>,
               tx_item: SyncSender<(Event, EventArg)>) -> Self {
        CachedSender{
            items: Vec::new(),
            rx_sender: rx_sender,
            tx_item: tx_item,
        }
    }

    pub fn run(&mut self) {
        // main loop for sending objects

        // if sender is not running, no need to send the items.
        let mut am_i_runing = false;
        // if the reader stopped, no need to wait for more items.
        let mut reader_stopped = false;
        let mut index = 0;

        loop {
            if let Ok((ev, arg)) = self.rx_sender.try_recv() {
                match ev {
                    Event::EvReaderStarted => {
                        // pass the event to matcher
                        let _ = self.tx_item.send((ev, arg));

                        reader_stopped = false;
                        self.items.clear();
                    }

                    Event::EvReaderStopped => {
                        // pass the event to matcher
                        let _ = self.tx_item.send((ev, arg));

                        reader_stopped = true;
                    }

                    Event::EvSenderRestart => {
                        // pass the event to matcher, it includes the query
                        let _ = self.tx_item.send((Event::EvMatcherRestart, arg));

                        am_i_runing = true;
                        index = 0;
                    }

                    Event::EvReaderNewItem => {
                        self.items.push(*arg.downcast::<Item>().unwrap());
                    }

                    _ => {}
                }
            }

            // send some data if running
            if am_i_runing {
                // there are more items to be sent
                if index < self.items.len() {
                    let _ = self.tx_item.send((Event::EvMatcherNewItem, Box::new(self.items[index].clone())));
                    index += 1;
                } else if reader_stopped {
                    let _ = self.tx_item.send((Event::EvSenderStopped, Box::new(true)));
                    am_i_runing = false;
                }
            } else {
                thread::sleep(Duration::from_millis(3));
            }
        }
    }
}

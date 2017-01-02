use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender};
use item::{Item, ItemGroup};
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

const SENDER_BATCH_SIZE: usize = 9997;

// sender is a cache of reader
pub struct CachedSender {
    items: Vec<ItemGroup>, // cache
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
            let mut item_group = Vec::new();

            // try to read a bunch of items first
            for _ in 0..SENDER_BATCH_SIZE {
                if let Ok((ev, arg)) = self.rx_sender.try_recv() {
                    match ev {
                        Event::EvReaderStarted => {
                            // pass the event to matcher
                            let _ = self.tx_item.send((ev, arg));

                            reader_stopped = false;
                            self.items.clear();
                        }

                        Event::EvReaderStopped => {
                            // send the total number that reader read.
                            let total_num: usize = self.items.iter().map(|group| group.len()).sum();
                            let _ = self.tx_item.send((ev, Box::new(total_num)));

                            reader_stopped = true;
                        }

                        Event::EvSenderRestart => {
                            // pass the event to matcher, it includes the query
                            let _ = self.tx_item.send((Event::EvMatcherRestart, arg));

                            am_i_runing = true;
                            index = 0;
                        }

                        Event::EvReaderNewItem => {
                            //self.items.push(Arc::new(*arg.downcast::<Item>().unwrap()));
                            item_group.push(Arc::new(*arg.downcast::<Item>().unwrap()));
                        }

                        _ => {}
                    }
                } else {
                    break;
                }
            }

            if !item_group.is_empty() {
                self.items.push(item_group);
            }

            if am_i_runing {
                // try to send a bunch of items:
                if index < self.items.len() {
                    if let Ok(_) = self.tx_item.try_send((Event::EvMatcherNewItem, Box::new(self.items[index].clone()))) {
                        index += 1;
                    }
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

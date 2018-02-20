use std::sync::mpsc::SyncSender;
use item::ItemGroup;
use event::{Event, EventArg, EventReceiver};
use std::thread;
use std::time::Duration;

// sender is a cache of reader
pub struct CachedSender {
    items: Vec<ItemGroup>, // cache
    rx_sender: EventReceiver,
    tx_item: SyncSender<(Event, EventArg)>,
}

impl CachedSender {
    pub fn new(rx_sender: EventReceiver, tx_item: SyncSender<(Event, EventArg)>) -> Self {
        CachedSender {
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
            // try to read a bunch of items first
            if let Ok((ev, arg)) = self.rx_sender.try_recv() {
                match ev {
                    Event::EvReaderStarted => {
                        reader_stopped = false;
                        self.items.clear();
                        index = 0;
                        am_i_runing = true;
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

                        if !reader_stopped {
                            // pass the event to matcher
                            let _ = self.tx_item.send((Event::EvReaderStarted, Box::new(true)));
                        }

                        am_i_runing = true;
                        index = 0;
                    }

                    Event::EvReaderNewItem => {
                        self.items.push(*arg.downcast::<ItemGroup>()
                            .expect("sender:EvReaderNewItem: failed to get argument"));
                    }

                    _ => {}
                }
            }

            if am_i_runing {
                // try to send a bunch of items:
                if index < self.items.len() {
                    if self.tx_item
                        .try_send((Event::EvMatcherNewItem, Box::new(self.items[index].clone())))
                        .is_ok()
                    {
                        index += 1;
                    }
                } else if reader_stopped {
                    let _ = self.tx_item.send((Event::EvSenderStopped, Box::new(true)));
                    am_i_runing = false;
                }
            } else {
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

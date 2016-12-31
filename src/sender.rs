use std::sync::mpsc::{Receiver, Sender, SyncSender, channel};
use std::error::Error;
use item::Item;
use std::sync::{Arc, RwLock};
use std::process::{Command, Stdio, Child};
use std::io::{stdin, BufRead, BufReader};
use event::{Event, EventArg};
use std::thread::{spawn, JoinHandle};
use std::thread;
use std::time::{Instant, Duration};
use std::collections::HashMap;

use std::io::Write;
use getopts;
use regex::Regex;

macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

const SENDER_BULK: usize = 100;

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

        let mut debug_timer = None;
        loop {
            if let Ok((ev, arg)) = self.rx_sender.try_recv() {
                match ev {
                    Event::EvReaderStarted => {
                        // pass the event to matcher
                        self.tx_item.send((ev, arg));

                        reader_stopped = false;
                        self.items.clear();
                    }

                    Event::EvReaderStopped => {
                        // pass the event to matcher
                        self.tx_item.send((ev, arg));

                        reader_stopped = true;
                    }

                    Event::EvSenderRestart => {
                        // pass the event to matcher, it includes the query
                        self.tx_item.send((Event::EvMatcherRestart, arg));

                        am_i_runing = true;
                        index = 0;
                        debug_timer = Some(Instant::now());
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
                    //let upper = min(self.items.len(), index+SENDER_BULK);
                    //for item in self.items[index .. upper] {
                        //self.tx_item.send((Event::EvMatcherNewItem, Box::new(item.clone())));
                    //}
                    //index = upper;

                    self.tx_item.send((Event::EvMatcherNewItem, Box::new(self.items[index].clone())));
                    index += 1;

                } else if reader_stopped {
                    self.tx_item.send((Event::EvSenderStopped, Box::new(true)));
                    am_i_runing = false;

                    let time = debug_timer.take().map(|t| t.elapsed()).unwrap_or(Duration::from_millis(0));
                    let mills = (time.as_secs()*1000) as u32 + time.subsec_nanos()/1000/1000;
                    //println_stderr!("sender spend time: {}ms", mills);
                }
            }
        }
    }
}

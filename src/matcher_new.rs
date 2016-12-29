use std::sync::mpsc::{Receiver, Sender};
use event::{Event, EventArg};
use item::Item;

pub struct Matcher {
    tx_result: Sender<(Event, EventArg)>,
    rx_item: Receiver<(Event, EventArg)>,
}

impl Matcher {
    pub fn new(rx_item: Receiver<(Event, EventArg)>, tx_result: Sender<(Event, EventArg)>) -> Self {
        Matcher {
            rx_item: rx_item,
            tx_result: tx_result,
        }
    }


    pub fn run(&self) {
        let mut query = "".to_string();
        while let Ok((ev, arg)) = self.rx_item.recv() {
            match ev {
                Event::EvMatcherNewItem => {
                    let item = *arg.downcast::<Item>().unwrap();

                    // TODO: filter logic

                    if query == "" {
                        self.tx_result.send((Event::EvModelNewItem, Box::new(item)));
                    } else if item.text.starts_with(&query) {
                        self.tx_result.send((Event::EvModelNewItem, Box::new(item)));
                    }
                }

                Event::EvMatcherRestart => {
                    query = *arg.downcast::<String>().unwrap();

                    // notifiy the model that the query had been changed
                    self.tx_result.send((Event::EvModelRestart, Box::new(true)));
                }

                _ => {}
            }
        }
    }
}

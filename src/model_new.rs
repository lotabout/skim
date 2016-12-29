use std::sync::mpsc::{Receiver, Sender, channel};
use event::{Event, EventArg};
use item::Item;
use std::thread;
use std::time::Duration;

pub struct Model {
    rx_cmd: Receiver<(Event, EventArg)>,
    items: Vec<Item>, // all items
    total_item: usize,

    item_cursor: usize, // the index of matched item currently highlighted.
    line_cursor: usize, // line No.
    hscroll_offset: usize,
}

impl Model {
    pub fn new(rx_cmd: Receiver<(Event, EventArg)>) -> Self {
        Model {
            rx_cmd: rx_cmd,
            items: Vec::new(),
            total_item: 0,

            item_cursor: 0,
            line_cursor: 0,
            hscroll_offset: 0,
        }
    }

    pub fn run(&mut self) {
        let (tx, rx): (Sender<bool>, Receiver<bool>) = channel();

        // start a timer for notifying refresh
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_millis(2000));
                tx.send(true);
            }
        });

        // main loop
        loop {
            // check for new item
            if let Ok((ev, arg)) = self.rx_cmd.try_recv() {
                match ev {
                    Event::EvMatcherNewItem => {
                        let item = *arg.downcast::<Item>().unwrap();
                        self.new_item(item);
                    }

                    Event::EvMatcherRestart => {
                        // clean the model
                        self.clean_model();
                    }

                    _ => {}
                }
            }

            // check if we need to update the view
            if let Ok(refresh) = rx.try_recv() {
                self.print_screen();
            }
        }
    }

    fn clean_model(&mut self) {
        self.items.clear();
        self.total_item = 0;
        self.item_cursor = 0;
        self.line_cursor = 0;
        self.hscroll_offset = 0;
    }

    fn new_item(&mut self, item: Item) {
        self.items.push(item);
    }

    fn print_screen(&self) {
        for item in self.items.iter() {
            println!("{:?}", item);
        }
    }
}

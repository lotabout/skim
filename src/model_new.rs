use std::sync::mpsc::{Receiver, Sender, channel};
use event::{Event, EventArg};
use item::Item;
use std::thread;
use std::time::Duration;
use std::io::{Write, stdout, Stdout};
use std::fmt;
use std::cmp::{max, min};

use curses::{ColorTheme, Curses};
use curses;

macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

pub type ClosureType = Box<Fn(&Curses) + Send>;

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
        // generate a new instance of curses for printing

        let curses = Curses::new();
        let theme = ColorTheme::new();
        curses::init(Some(&theme), false, false);

        // main loop
        loop {
            // check for new item
            if let Ok((ev, arg)) = self.rx_cmd.try_recv() {
                match ev {
                    Event::EvModelNewItem => {
                        let item = *arg.downcast::<Item>().unwrap();
                        self.new_item(item);
                    }

                    Event::EvModelRestart => {
                        // clean the model
                        self.clean_model();
                    }

                    Event::EvModelRedraw => {
                        let hook = *arg.downcast::<ClosureType>().unwrap();

                        curses.clear();
                        self.print_screen(&curses);
                        hook(&curses);
                        curses.refresh();
                    }

                    Event::EvActAccept => {
                        let tx_ack = *arg.downcast::<Sender<bool>>().unwrap();

                        curses.close();

                        tx_ack.send(true);
                    }

                    _ => {}
                }
            }
        }

        curses.close();
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

    fn print_screen(&mut self, curses: &Curses) {
        let (width, height) = curses.get_maxyx();
        let (width, height) = (width as usize, height as usize);

        for (l, item) in self.items[0 .. min(height-1, self.items.len())].iter().enumerate() {
            curses.mv(l as i32, 0);
            curses.printw("  ");
            curses.printw(&item.text);
        }
    }
}

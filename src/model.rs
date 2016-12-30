use std::sync::mpsc::{Receiver, Sender, channel};
use event::{Event, EventArg};
use item::{MatchedItem, Item};
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
    items: Vec<MatchedItem>, // all items
    total_item: usize,

    item_cursor: usize, // the index of matched item currently highlighted.
    line_cursor: usize, // line No.
    hscroll_offset: usize,
    reverse: bool,
    height: i32,
    width: i32,
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
            reverse: false,
            height: 0,
            width: 0,
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
                        let item = *arg.downcast::<MatchedItem>().unwrap();
                        self.new_item(item);
                    }

                    Event::EvModelRestart => {
                        // clean the model
                        self.clean_model();
                        self.update_size(&curses);
                    }

                    Event::EvModelRedraw => {
                        self.update_size(&curses);

                        let print_query = *arg.downcast::<ClosureType>().unwrap();
                        curses.clear();
                        self.print_screen(&curses, print_query);
                        curses.refresh();
                    }

                    Event::EvActAccept => {
                        let tx_ack = *arg.downcast::<Sender<bool>>().unwrap();

                        curses.close();

                        // TODO: output the selected items

                        tx_ack.send(true);
                    }
                    Event::EvActUp => {
                        self.act_move_line_cursor(1);
                    }
                    Event::EvActDown => {
                        let (h, w) = curses.get_maxyx();
                        self.act_move_line_cursor(-1);
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

    fn update_size(&mut self, curses: &Curses) {
        // update the (height, width)
        let (h, w) = curses.get_maxyx();
        self.height = h-1;
        self.width = w-2;
    }

    fn new_item(&mut self, item: MatchedItem) {
        self.items.push(item);
    }

    fn print_screen(&mut self, curses: &Curses, print_query: ClosureType) {
        let (h, w) = curses.get_maxyx();
        let (h, w) = (h as usize, w as usize);

        // screen-line: y         <--->   item-line: (height - y - 1)
        //              h-1                          h-(h-1)-1 = 0
        //              0                            h-1
        // screen-line: (h-l-1)   <--->   item-line: l

        let lower = self.item_cursor;
        let upper = min(self.item_cursor + h-1, self.items.len());

        for (l, item) in self.items[lower .. upper].iter().enumerate() {
            curses.mv((if self.reverse {l+1} else {h-2 - l} ) as i32, 0);

            // print a single item
            if l == self.line_cursor {
                curses.printw(">");
            } else {
                curses.printw(" ");
            }
            curses.printw(" ");
            curses.printw(&(item.item.text));
        }

        // print query
        curses.mv((if self.reverse {0} else {h-1}) as i32, 0);
        print_query(curses);
    }


    pub fn act_move_line_cursor(&mut self, diff: i32) {
        let diff = if self.reverse {-diff} else {diff};
        let mut line_cursor = self.line_cursor as i32;
        let mut item_cursor = self.item_cursor as i32;
        let item_len = self.items.len() as i32;

        line_cursor += diff;
        if line_cursor >= self.height {
            item_cursor += line_cursor - self.height + 1;
            item_cursor = max(0, min(item_cursor, item_len - self.height));
            line_cursor = min(self.height-1, item_len - item_cursor);
        } else if line_cursor < 0 {
            item_cursor += line_cursor;
            item_cursor = max(item_cursor, 0);
            line_cursor = 0;
        } else {
            line_cursor = min(line_cursor, item_len-1 - item_cursor);
        }

        self.item_cursor = item_cursor as usize;
        self.line_cursor = line_cursor as usize;
    }
}

use std::sync::mpsc::{Receiver, Sender, channel};
use event::{Event, EventArg};
use item::Item;
use std::thread;
use std::time::Duration;
use termion::raw::{RawTerminal, IntoRawMode};
use std::io::{Write, stdout, Stdout};
use termion::{clear, cursor, terminal_size};
use std::fmt;
use std::cmp::{max, min};

macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);


pub struct Model {
    rx_cmd: Receiver<(Event, EventArg)>,
    items: Vec<Item>, // all items
    total_item: usize,

    item_cursor: usize, // the index of matched item currently highlighted.
    line_cursor: usize, // line No.
    hscroll_offset: usize,
    stdout: RawTerminal<Stdout>,
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
            stdout: stdout().into_raw_mode().unwrap(),
        }
    }

    pub fn run(&mut self) {
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
                        self.print_screen();
                    }

                    _ => {}
                }
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

    fn print_screen(&mut self) {
        let (width, height) = terminal_size().unwrap();
        let (width, height) = (width as usize, height as usize);

        for (l, item) in self.items[0 .. min(height-1, self.items.len())].iter().enumerate() {
            write!(self.stdout, "{}{}", cursor::Goto(3, (l+1) as u16), clear::CurrentLine);
            write!(self.stdout, "{}", item.text);
        }
        self.stdout.flush().unwrap();
    }
}

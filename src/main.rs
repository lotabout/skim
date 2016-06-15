extern crate libc;
extern crate ncurses;

mod util;
mod item;

use std::io::{stdin, Read, BufRead, BufReader};
use std::error::Error;
use std::sync::{Arc, RwLock};
use std::thread;
use std::process::{Command, Stdio};
use std::char;
use std::cmp;
use std::sync::mpsc::{Sender, channel};
use util::eventbox::EventBox;

use ncurses::*;

use item::{Item, MatchedItem};

//==============================================================================
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum Event{
    EvReaderNewItem,
    EvReaderFinished,
    EvMatcherNewItem,
    EvMatcherResetQuery,
    EvMatcherUpdateProcess,
    EvQueryChange,
    EvInputToggle,
    EvInputUp,
    EvInputDown,
    EvInputSelect,
}

// matcher will receive two events:
// 1. EvMatcherNewItem, to reset the input strings
// 2. EvMatcherResetQuery, to interrupt current processing.
//
// will send two events:
// 1. EvMatcherUpdateProcess, to notify the matched/total items
// 2. EvMatcherFinished.

struct Matcher {
    tx_output: Sender<MatchedItem>,   // channel to send output to
    eb_req: Arc<EventBox<Event>>,       // event box that recieve requests
    eb_notify: Arc<EventBox<Event>>,    // event box that send out notification
    items: Arc<RwLock<Vec<Item>>>,
    item_pos: usize,
    num_matched: u64,
    query: String,
}


impl Matcher {
    pub fn new(items: Arc<RwLock<Vec<Item>>>, tx_output: Sender<MatchedItem>,
               eb_req: Arc<EventBox<Event>>, eb_notify: Arc<EventBox<Event>>) -> Self {
        Matcher {
            tx_output: tx_output,
            eb_req: eb_req,
            eb_notify: eb_notify,
            items: items,
            item_pos: 0,
            num_matched: 0,
            query: String::new(),
        }
    }

    fn match_str(&self, item: &str) -> bool {
        if self.query == "" {
            return true;
        }

        item.starts_with(&self.query)
    }

    pub fn process(&mut self) {
        let items = self.items.read().unwrap();
        for item in items[self.item_pos..].into_iter() {
            // process the matcher
            //self.tx_output.send(string.clone());
            if self.match_str(&item.text) {
                self.num_matched += 1;
                let _ = self.tx_output.send(MatchedItem::new(self.item_pos));
            }


            (*self.eb_notify).set(Event::EvMatcherUpdateProcess, Box::new((self.num_matched, items.len() as u64)));

            self.item_pos += 1;
            if (self.item_pos % 100) == 99 && !self.eb_req.is_empty() {
                break;
            }
        }
    }

    fn reset_query(&mut self, query: &str) {
        self.query.clear();
        self.query.push_str(query);
        self.num_matched = 0;
        self.item_pos = 0;
    }

    pub fn run(&mut self) {
        loop {
            for (e, val) in (*self.eb_req).wait() {
                match e {
                    Event::EvMatcherNewItem => {}
                    Event::EvMatcherResetQuery => {
                        self.reset_query(&val.downcast::<String>().unwrap());
                    }
                    _ => {}
                }
            }

            self.process()
        }
    }
}

//==============================================================================
// Input: fetch the query string, handle key event

struct Input {
    query: Vec<char>,
    index: usize, // index in chars
    pos: usize, // position in bytes
    eb: Arc<EventBox<Event>>,
}

impl Input {
    pub fn new(eb: Arc<EventBox<Event>>) -> Self {
        Input {
            query: Vec::new(),
            index: 0,
            pos: 0,
            eb: eb,
        }
    }

    fn get_query(&self) -> String {
        self.query.iter().cloned().collect::<String>()
    }

    fn add_char (&mut self, ch: char) {
        self.query.insert(self.index, ch);
        self.index += 1;
        self.pos += if ch.len_utf8() > 1 {2} else {1};
    }

    fn delete_char(&mut self) {
        if self.index == 0 {
            return;
        }

        let ch = self.query.remove(self.index-1);
        self.index -= 1;
        self.pos -= if ch.len_utf8() > 1 {2} else {1};
    }

    pub fn run(&mut self) {
        loop {
            self.handle_char();
        }
    }

    // fetch input from curses and turn it into query.
    fn handle_char(&mut self) {
        let ch = wget_wch(stdscr);

        match ch {
            Some(WchResult::KeyCode(_)) => {
                // will later handle readline-like shortcuts
            }

            Some(WchResult::Char(c)) => {
                /* Enable attributes and output message. */
                let ch = char::from_u32(c as u32).expect("Invalid char");
                match ch {
                    '\x7F' => { // backspace
                        self.delete_char();
                        self.eb.set(Event::EvQueryChange, Box::new((self.get_query(), self.pos)));
                    }

                    '\x0A' => { // enter
                        self.eb.set(Event::EvInputSelect, Box::new(true));
                    }

                    '\x09' => { // tab
                        self.eb.set(Event::EvInputToggle, Box::new(true));
                    }

                    '\x10' => { // ctrl-p
                        self.eb.set(Event::EvInputUp, Box::new(true));
                    }

                    '\x0E' => { // ctrl-n
                        self.eb.set(Event::EvInputDown, Box::new(true));
                    }

                    ch => { // other characters
                        self.add_char(ch);
                        self.eb.set(Event::EvQueryChange, Box::new((self.get_query(), self.pos)));
                    }
                }
            }

            None => { }
        }
    }
}

//==============================================================================
// Reader: fetch a list of lines from stdin or command output

struct Reader {
    cmd: Option<&'static str>, // command to invoke
    eb: Arc<EventBox<Event>>,         // eventbox
    tx: Sender<Item>,    // sender to send the string read from command output
}

impl Reader {

    pub fn new(cmd: Option<&'static str>, eb: Arc<EventBox<Event>>, tx: Sender<Item>) -> Self {
        Reader{cmd: cmd, eb: eb, tx: tx}
    }

    // invoke find comand.
    fn get_command_output(&self) -> Result<Box<BufRead>, Box<Error>> {
        let command = try!(Command::new("sh")
                           .arg("-c")
                           .arg(self.cmd.unwrap_or("find ."))
                           .stdout(Stdio::piped())
                           .stderr(Stdio::null())
                           .spawn());
        let stdout = try!(command.stdout.ok_or("command output: unwrap failed".to_owned()));
        Ok(Box::new(BufReader::new(stdout)))
    }

    fn run(&mut self) {
        // check if the input is TTY
        let istty = unsafe { libc::isatty(libc::STDIN_FILENO as i32) } != 0;

        let mut read;
        if istty {
            read = self.get_command_output().expect("command not found");
        } else {
            read = Box::new(BufReader::new(stdin()))
        };

        loop {
            let mut input = String::new();
            match read.read_line(&mut input) {
                Ok(n) => {
                    if n <= 0 { break; }

                    if input.ends_with("\n") {
                        input.pop();
                        if input.ends_with("\r") {
                            input.pop();
                        }
                    }
                    let _ = self.tx.send(Item::new(input));
                }
                Err(_err) => { break; }
            }
            self.eb.set(Event::EvReaderNewItem, Box::new(0));
        }
        self.eb.set(Event::EvReaderFinished, Box::new(0));
    }
}

//==============================================================================
// Model: data structure for display the result
struct Model {
    query: String,
    query_cursor: i32,  // > qu<query_cursor>ery
    num_matched: u64,
    num_total: u64,
    items: Arc<RwLock<Vec<Item>>>, // all items
    matched_items: Vec<MatchedItem>,
    item_cursor: usize, // the index of matched item currently highlighted.
    line_cursor: usize, // line No.
    item_start_pos: usize, // for screen scroll.
    max_y: i32,
    max_x: i32,
}

impl Model {
    pub fn new() -> Self {
        let mut max_y = 0;
        let mut max_x = 0;
        getmaxyx(stdscr, &mut max_y, &mut max_x);

        Model {
            query: String::new(),
            query_cursor: 0,
            num_matched: 0,
            num_total: 0,
            items: Arc::new(RwLock::new(Vec::new())),
            matched_items: Vec::new(),
            item_cursor: 0,
            line_cursor: (max_y - 3) as usize,
            item_start_pos: 0,
            max_y: max_y,
            max_x: max_x,
        }
    }

    pub fn output(&self) {
        let items = self.items.read().unwrap();
        for item in items.iter() {
            if item.selected {
                println!("{}", item.text);
            }
        }
        //println!("{:?}", items[self.matched_items[self.item_cursor].index].text);
        //items[self.matched_items[self.item_cursor].index].selected = s;
    }

    pub fn toggle_select(&self, selected: Option<bool>) {
        let mut items = self.items.write().unwrap();
        items[self.matched_items[self.item_cursor].index].toggle_select(selected);
    }

    pub fn update_query(&mut self, query: String, cursor: i32) {
        self.query = query;
        self.query_cursor = cursor;
    }

    pub fn update_process_info(&mut self, matched: u64, total: u64) {
        self.num_matched = matched;
        self.num_total = total;
    }

    pub fn push_item(&mut self, item: MatchedItem) {
        self.matched_items.push(item);
    }

    pub fn clear_items(&mut self) {
        self.matched_items.clear();
    }

    pub fn move_line_cursor(&mut self, diff: i32) {

        let y = self.line_cursor as i32 + diff;
        let item_y = cmp::max(0, self.item_cursor as i32 - diff);
        let screen_height = (self.max_y - 3) as usize;

        match y {
            y if y < 0 => {
                self.line_cursor = 0;
                self.item_cursor = cmp::min(item_y as usize, self.matched_items.len()-1);
                self.item_start_pos = self.item_cursor - screen_height;
            }

            y if y > screen_height as i32 => {
                self.line_cursor = screen_height;
                self.item_cursor = cmp::max(0, item_y as usize);
                self.item_start_pos = self.item_cursor;
            }

            y => {
                self.line_cursor = y as usize;
                self.item_cursor = item_y as usize;
            }
        }
    }

    pub fn print_query(&self) {
        // > query
        mv(self.max_y-1, 0);
        addstr("> ");
        addstr(&self.query);
        mv(self.max_y-1, self.query_cursor+2);
    }

    pub fn print_info(&self) {
        mv(self.max_y-2, 0);
        addstr(format!("  {}/{}", self.num_matched, self.num_total).as_str());
    }

    fn print_item(&self, item: &Item) {
        let shown_str: String = item.text.chars().take((self.max_x-1) as usize).collect();
        if item.selected {
            printw(">");
        } else {
            printw(" ");
        }

        addstr(&shown_str);
    }

    pub fn print_items(&self) {
        let items = self.items.read().unwrap();

        let mut y = self.max_y - 3;
        for matched in self.matched_items[self.item_start_pos..].into_iter() {
            mv(y, 0);
            let is_current_line = y == self.line_cursor as i32;

            if is_current_line {
                printw(">");
            } else {
                printw(" ");
            }

            self.print_item(&items[matched.index]);

            y -= 1;
            if y < 0 {
                break;
            }
        }
    }

    pub fn refresh(&self) {
        refresh();
    }

    pub fn display(&self) {
        erase();
        self.print_items();
        self.print_info();
        self.print_query();
        self.refresh();
    }
}

//==============================================================================
// Display: for printing the result

//==============================================================================

fn main() {
    // initialize ncurses
    let local_conf = LcCategory::all;
    setlocale(local_conf, "en_US.UTF-8"); // for showing wide characters
    initscr();
    raw();
    keypad(stdscr, true);
    noecho();

    let mut model = Model::new();

    let eb = Arc::new(EventBox::new());
    let (tx_source, rx_source) = channel();
    let (tx_matched, rx_matched) = channel();

    let eb_clone_reader = eb.clone();
    let mut reader = Reader::new(Some(&"find ."), eb_clone_reader, tx_source);

    let eb_matcher = Arc::new(EventBox::new());
    let eb_matcher_clone = eb_matcher.clone();
    let eb_clone_matcher = eb.clone();
    let items = model.items.clone();
    let mut matcher = Matcher::new(items, tx_matched, eb_matcher_clone, eb_clone_matcher);

    let eb_clone_input = eb.clone();
    let mut input = Input::new(eb_clone_input);

    // start running
    thread::spawn(move || {
        reader.run();
    });

    thread::spawn(move || {
        matcher.run();
    });

    thread::spawn(move || {
        input.run();
    });

    'outer: loop {
        for (e, val) in eb.wait() {
            match e {
                Event::EvReaderNewItem | Event::EvReaderFinished => {
                    let mut items = model.items.write().unwrap();
                    while let Ok(string) = rx_source.try_recv() {
                        items.push(string);
                    }
                    eb_matcher.set(Event::EvMatcherNewItem, Box::new(true));
                }

                Event::EvMatcherUpdateProcess => {
                    let (matched, total) : (u64, u64) = *val.downcast().unwrap();
                    model.update_process_info(matched, total);

                    while let Ok(matched_item) = rx_matched.try_recv() {
                        model.push_item(matched_item);
                    }
                }

                Event::EvQueryChange => {
                    let (query, pos) : (String, usize) = *val.downcast().unwrap();
                    let modified = query != model.query;
                    model.update_query(query.clone(), pos as i32);

                    if modified {
                        model.clear_items();
                        eb_matcher.set(Event::EvMatcherResetQuery, Box::new(model.query.clone()));
                    }
                }

                Event::EvInputSelect => {
                    // break out of the loop and output the selected item.
                    break 'outer;
                }

                Event::EvInputToggle => {
                    model.toggle_select(None);
                    model.move_line_cursor(1);
                }
                Event::EvInputUp=> {
                    model.move_line_cursor(-1);
                }
                Event::EvInputDown=> {
                    model.move_line_cursor(1);
                }

                _ => {
                    printw(format!("{}\n", e as i32).as_str());
                }
            }
        }
        model.display();
        refresh();
    };

    endwin();
    model.output();
}

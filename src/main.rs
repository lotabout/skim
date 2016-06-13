extern crate libc;
extern crate ncurses;

mod util;

use std::io::{stdin, Read, BufRead, BufReader};
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::process::{Command, Stdio, exit};
use std::char;
use std::mem;
use std::sync::mpsc::{Sender, Receiver, channel};
use util::eventbox::EventBox;

use ncurses::*;

//==============================================================================

struct FZF {
    query: String,
}

//==============================================================================
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
enum Event{
    EvReaderNewItem,
    EvReaderFinished,
    EvMatcherNewItem,
    EvMatcherResetQuery,
    EvMatcherUpdateProcess,
    EvMatcherFinished,
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
    rx_source: Receiver<String>, // channel to retrieve strings from reader
    tx_output: Sender<String>,   // channel to send output to
    eb_req: Arc<EventBox<Event>>,       // event box that recieve requests
    eb_notify: Arc<EventBox<Event>>,    // event box that send out notification
    items: Vec<String>,
    item_pos: usize,
    num_matched: u64,
    query: String,
}


impl Matcher {
    pub fn new(rx_source: Receiver<String>, tx_output: Sender<String>,
               eb_req: Arc<EventBox<Event>>, eb_notify: Arc<EventBox<Event>>) -> Self {
        Matcher {
            rx_source: rx_source,
            tx_output: tx_output,
            eb_req: eb_req,
            eb_notify: eb_notify,
            items: Vec::new(),
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
        for string in self.items[self.item_pos..].into_iter() {
            // process the matcher
            //self.tx_output.send(string.clone());
            if self.match_str(string) {
                self.num_matched += 1;
                self.tx_output.send(string.clone());
            }


            (*self.eb_notify).set(Event::EvMatcherUpdateProcess, Box::new((self.num_matched, self.items.len() as u64)));

            self.item_pos += 1;
            if (self.item_pos % 100) == 99 && !self.eb_req.is_empty() {
                break;
            }
        }
    }

    fn read_new_item(&mut self) {
        while let Ok(string) = self.rx_source.try_recv() {
            self.items.push(string);
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
                    Event::EvMatcherNewItem => { self.read_new_item();}
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

        let orig_query = self.query.clone();

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

const READER_LINES_CACHED: usize = 100;

struct Reader {
    cmd: Option<&'static str>, // command to invoke
    eb: Arc<EventBox<Event>>,         // eventbox
    tx: Sender<String>,    // sender to send the string read from command output
}



impl Reader {

    pub fn new(cmd: Option<&'static str>, eb: Arc<EventBox<Event>>, tx: Sender<String>) -> Self {
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
                    self.tx.send(input);
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
    query_cursor: i32,
    num_matched: u64,
    num_total: u64,
    matched_items: Vec<String>,
    item_start_pos: i64,
    line_cursor: i32,
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
            matched_items: Vec::new(),
            item_start_pos: 0,
            line_cursor: 0,
            max_y: max_y,
            max_x: max_x,
        }
    }

    pub fn update_query(&mut self, query: String, cursor: i32) {
        self.query = query;
        self.query_cursor = cursor;
    }

    pub fn update_process_info(&mut self, matched: u64, total: u64) {
        self.num_matched = matched;
        self.num_total = total;
    }

    pub fn push_item(&mut self, item: String) {
        self.matched_items.push(item);
    }

    pub fn clear_items(&mut self) {
        self.matched_items.clear();
    }

    pub fn move_line_cursor(&mut self, diff: i32) {
        self.line_cursor += diff;
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

    pub fn print_items(&self) {
        let mut y = self.max_y - 2;
        for item in self.matched_items.iter() {
            mv(y, 2);

            let shown_str: String = item.chars().take((self.max_x-1) as usize).collect();
            addstr(&shown_str);

            y -= 1;
            if y <= 0 {
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
    let mut matcher = Matcher::new(rx_source, tx_matched, eb_matcher_clone, eb_clone_matcher);

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
                    eb_matcher.set(Event::EvMatcherNewItem, Box::new(true));
                }

                Event::EvMatcherUpdateProcess => {
                    let (matched, total) : (u64, u64) = *val.downcast().unwrap();
                    model.update_process_info(matched, total);

                    while let Ok(string) = rx_matched.try_recv() {
                        model.push_item(string);
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

                _ => {
                    printw(format!("{}\n", e as i32).as_str());
                }
            }
        }
        model.display();
        refresh();
    };

    endwin();
}

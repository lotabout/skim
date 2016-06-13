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
    EV_READER_NEW,
    EV_READER_FIN,
    EV_MATCHER_NEW_ITEM,
    EV_MATCHER_RESET_QUERY,
    EV_MATCHER_UPDATE_PROCESS,
    EV_MATCHER_FINISHED,
}

// matcher will receive two events:
// 1. EV_MATCHER_NEW_ITEM, to reset the input strings
// 2. EV_MATCHER_RESET_QUERY, to interrupt current processing.
//
// will send two events:
// 1. EV_MATCHER_UPDATE_PROCESS, to notify the matched/total items
// 2. EV_MATCHER_FINISHED.

struct Matcher {
    rx_source: Receiver<String>, // channel to retrieve strings from reader
    tx_output: Sender<String>,   // channel to send output to
    eb_req: Arc<EventBox<Event>>,       // event box that recieve requests
    eb_notify: Arc<EventBox<Event>>,    // event box that send out notification
    items: Vec<String>,
    item_pos: usize,
    query: String,
}
    

impl Matcher {
    pub fn new(rx_source: Receiver<String>, tx_output: Sender<String>,
               eb_req: Arc<EventBox<Event>>, eb_notify: Arc<EventBox<Event>>) -> Self {
        Matcher{
            rx_source: rx_source,
            tx_output: tx_output,
            eb_req: eb_req,
            eb_notify: eb_notify,
            items: Vec::new(),
            item_pos: 0,
            query: String::new(),
        }
    }

    pub fn process(&mut self) {
        for string in self.items[self.item_pos..].into_iter() {
            // process the matcher
            //self.tx_output.send(string.clone());
            (*self.eb_notify).set(Event::EV_MATCHER_UPDATE_PROCESS, Box::new(0));
            self.tx_output.send(string.clone());

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
    }

    pub fn run(&mut self) {
        loop {
            for (e, val) in (*self.eb_req).wait() {
                match e {
                    Event::EV_MATCHER_NEW_ITEM => { self.read_new_item();}
                    Event::EV_MATCHER_RESET_QUERY => {self.reset_query(*val.downcast().unwrap());}
                    _ => {}
                }
            }

            self.process()
        }
    }
}

//==============================================================================
struct Display {
    max_y: i32,
    max_x: i32,
}

impl Display {
    pub fn new() -> Self {
        let mut max_y = 0;
        let mut max_x = 0;
        getmaxyx(stdscr, &mut max_y, &mut max_x);

        Display {
            max_y: max_y,
            max_x: max_x,
        }
    }

    pub fn print_info(& self, msg: &str) {
        // XX/YY file(s)
        mv(self.max_y-2, 0);
        clrtoeol();
        addstr(msg);
    }

    pub fn print_query(&self, query: &str, cursor: i32) {
        // > query
        mv(self.max_y-1, 0);
        clrtoeol();
        printw("> ");
        printw(query);
        mv(self.max_y-1, cursor+2);
    }

    pub fn prepare_print_line(&self) {
        mv(self.max_y-2, 2);
    }

    pub fn print_line(&self, line: &str) {
        let y = getcury(stdscr);
        mv(y, 2);
        addstr(line);
        mv(y-1, 2);
    }

    pub fn refresh(&self) {
        refresh();
    }
}

//==============================================================================
// Queryer: fetch the query string


#[derive(Debug)]
struct Queryer {
    query: String,
    pos: u32, // point to the last character of the query string
}

impl Queryer {
    pub fn new() -> Self {
        Queryer {
            query: String::new(),
            pos: 0,
        }
    }

    fn add_char (&mut self, ch: char) {
        let orig = mem::replace(&mut self.query, String::new());
        self.query.push_str(orig.chars().take(self.pos as usize).collect::<String>().as_ref());
        self.query.push(ch);
        self.query.push_str(orig.chars().skip(self.pos as usize).collect::<String>().as_ref());
        self.pos += 1;
    }

    fn delete_char(&mut self) {
        if self.pos == 0 {
            return;
        }

        let orig = mem::replace(&mut self.query, String::new());
        self.query.push_str(orig.chars().take((self.pos-1) as usize).collect::<String>().as_ref());
        self.query.push_str(orig.chars().skip(self.pos as usize).collect::<String>().as_ref());
        self.pos -= 1;
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
                    '\x7F' => { self.delete_char(); } // backspace
                    ch => { self.add_char(ch); } // other characters
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
            self.eb.set(Event::EV_READER_NEW, Box::new(0));
        }
        self.eb.set(Event::EV_READER_FIN, Box::new(0));
    }
}

//==============================================================================

fn main() {
    // initialize ncurses
    let local_conf = LcCategory::all;
    setlocale(local_conf, "en_US.UTF-8"); // for showing wide characters
    initscr();
    raw();
    keypad(stdscr, true);
    noecho();


    let eb = Arc::new(EventBox::new());
    let eb_clone_reader = eb.clone();
    let eb_clone_matcher = eb.clone();
    let (tx_source, rx_source) = channel();
    let (tx_matched, rx_matched) = channel();
    let mut reader = Reader::new(Some(&"find ."), eb_clone_reader, tx_source);
    let eb_matcher = Arc::new(EventBox::new());
    let eb_matcher_clone = eb_matcher.clone();
    let mut matcher = Matcher::new(rx_source, tx_matched, eb_matcher_clone, eb_clone_matcher);

    // start running
    thread::spawn(move || {
        reader.run();
    });

    thread::spawn(move || {
        matcher.run();
    });

    let mut items = vec![];

    loop {
        for (e, val) in eb.wait() {
            match e {
                Event::EV_READER_NEW => {
                    //printw("READER_NEW!\n");
                    eb_matcher.set(Event::EV_MATCHER_NEW_ITEM, Box::new(0));
                }
                Event::EV_READER_FIN => {
                    //printw("READER_FIN\n");
                }
                Event::EV_MATCHER_UPDATE_PROCESS => {
                    printw(format!("UPDATE_PROCESS: {}\n", items.len()).as_str());
                    while let Ok(string) = rx_matched.try_recv() {
                        items.push(string);
                    }
                }
                _ => {
                    printw(format!("{}\n", e as i32).as_str());
                }
            }
        }

        // print items
        let mut y = 0;
        for string in items.iter() {
            mv(y, 0);
            printw(string);
            y += 1;
        }

        refresh();
    };

    //let disp = Display::new();
    //disp.init();
    //disp.print_info(&matcher);
    //disp.print_query(&matcher);
    //disp.refresh();

    //let mut queryer = Queryer::new();

    //loop {
        //queryer.handle_char();
        //addstr(&queryer.query);
        //addstr(format!("pos: {}", queryer.pos).as_ref());
        //addch('\n' as u64);
        //refresh();
    //}

    // displayer
    //let displayer_mtx_item = mtx_item.clone();
    //let displayer = thread::spawn(move || {
        //loop {
            //let mut items = displayer_mtx_item.lock().unwrap();
            //if (*items).len() > 0 {
                //println!("Got: {:?}", *items);
                //*items = vec![];
            //}
        //}
    //});

    //getch();
    //endwin();
    //th_reader.join();
    //displayer.join();
}

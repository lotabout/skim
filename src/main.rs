extern crate libc;
extern crate ncurses;

use std::io::{stdin, Read, BufRead, BufReader};
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread;
use std::process::{Command, Stdio, exit};
use std::char;
use std::mem;

use ncurses::*;

//==============================================================================

struct FZF {
    query: String,
}

//==============================================================================
enum MatcherEvent{
    QUERY_CHANG,
    SELECT,
}

struct Matcher {
    query: String,
    mutex_items: Arc<Mutex<Vec<String>>>,
    matches: Vec<String>,
    display: Display,
}

impl Matcher {
    pub fn new(mtx_items: Arc<Mutex<Vec<String>>>) -> Self {
        Matcher {
            query: "my query".to_owned(),
            mutex_items: mtx_items,
            matches: vec!["one".to_owned(), "two".to_owned()],
            display: Display::new(),
        }
    }

    pub fn display_items(&mut self) {
        let mut items = self.mutex_items.lock().unwrap();
        let maxc = self.display.max_x - 3;
        self.display.prepare_print_line();
        for item in (*items).iter() {
            self.display.print_line(item);
        }
    }

    pub fn display_info(&self) {
        // XX/YY file(s)
        self.display.print_info(format!("  {}/{} file(s)", 10, 20).as_ref());
    }

    pub fn run(&mut self) {

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

// invoke find comand.
fn get_command_output() -> Result<Box<BufRead>, Box<Error>> {
    let command = try!(Command::new("ls")
                       .arg(".")
                       .stdout(Stdio::piped())
                       .stderr(Stdio::null())
                       .spawn());
    let stdout = try!(command.stdout.ok_or("command output: unwrap failed".to_owned()));
    Ok(Box::new(BufReader::new(stdout)))
}

struct Reader {
    lines: Vec<String>,
    mutex_output: Arc<Mutex<Vec<String>>>,
}

impl Reader {
    pub fn new(mtx: Arc<Mutex<Vec<String>>>) -> Self {
        Reader {
            lines: vec![],
            mutex_output: mtx,
        }
    }

    fn update_output(&mut self) {
        let mut items = self.mutex_output.lock().unwrap();
        (*items).extend(mem::replace(&mut self.lines, vec![]));
    }

    fn run(&mut self) {
        // check if the input is TTY
        let istty = unsafe { libc::isatty(libc::STDIN_FILENO as i32) } != 0;

        let mut read;
        if istty {
            read = get_command_output().expect("command not found: find");
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

                    self.lines.push(input);
                    if self.lines.len() >= READER_LINES_CACHED {
                        self.update_output();
                    }

                }
                Err(_err) => { break; }
            }
        }
        self.update_output();
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

    // initialize ncurses screen
    let mtx_item = Arc::new(Mutex::new(vec![]));

    // reader
    let reader_mtx_item = mtx_item.clone();
    let mut reader = Reader::new(reader_mtx_item);
    let th_reader = thread::spawn(move || reader.run());

    let mut matcher = Matcher::new(mtx_item);
    loop {
        matcher.display_items();
        matcher.display.refresh();
        getch();
    }

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
    th_reader.join();
    //displayer.join();
}

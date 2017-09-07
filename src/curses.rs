// An abstract layer towards ncurses-rs, which provides keycode, color scheme support
// Modeled after fzf

use ncurses::*;
use getopts;
use std::sync::RwLock;
use std::collections::HashMap;
use libc::{STDIN_FILENO, STDERR_FILENO, fdopen, c_char};
use std::sync::mpsc::Receiver;
use std::io::{stdin, stdout, Write, Stdout, BufReader, BufRead};
use std::io::prelude::*;
use std::fs::File;
use termion::event::{Key, Event, MouseEvent};
use termion::input::{TermRead, MouseTerminal};
use termion::raw::{IntoRawMode, RawTerminal};
use termion::screen::{AlternateScreen, ToMainScreen};
use termion::cursor::DetectCursorPos;
use termion;
use std::cmp::{min, max};

//use std::io::Write;
macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

pub static COLOR_NORMAL:        i16 = 0;
pub static COLOR_PROMPT:        i16 = 1;
pub static COLOR_MATCHED:       i16 = 2;
pub static COLOR_CURRENT:       i16 = 3;
pub static COLOR_CURRENT_MATCH: i16 = 4;
pub static COLOR_SPINNER:       i16 = 5;
pub static COLOR_INFO:          i16 = 6;
pub static COLOR_CURSOR:        i16 = 7;
pub static COLOR_SELECTED:      i16 = 8;
pub static COLOR_HEADER:        i16 = 9;
static COLOR_USER:              i16 = 10;

lazy_static! {
    static ref COLOR_MAP: RwLock<HashMap<i16, attr_t>> = RwLock::new(HashMap::new());
    static ref FG: RwLock<i16> = RwLock::new(7);
    static ref BG: RwLock<i16> = RwLock::new(0);
    static ref USE_COLOR: RwLock<bool> = RwLock::new(true);
}

pub fn init(theme: Option<&ColorTheme>, is_black: bool, _use_mouse: bool) {
    return;
    // initialize ncurses
    let mut use_color = USE_COLOR.write().unwrap();

    if let Some(theme) = theme {
        let base_theme = if tigetnum("colors") >= 256 {
            DARK256
        } else {
            DEFAULT16
        };

        init_pairs(&base_theme, theme, is_black);
        *use_color = true;
    } else {
        *use_color = false;
    }
}

fn init_pairs(base: &ColorTheme, theme: &ColorTheme, is_black: bool) {
    let mut fg = FG.write().unwrap();
    let mut bg = BG.write().unwrap();


    *fg = shadow(base.fg, theme.fg);
    *bg = shadow(base.bg, theme.bg);

    if is_black {
        *bg = COLOR_BLACK;
    } else if theme.use_default {
        *fg = COLOR_DEFAULT;
        *bg = COLOR_DEFAULT;
        use_default_colors();
    }

    if !theme.use_default {
        assume_default_colors(shadow(base.fg, theme.fg) as i32, shadow(base.bg, theme.bg) as i32);
    }

    start_color();

    init_pair(COLOR_PROMPT,        shadow(base.prompt,        theme.prompt),        *bg);
    init_pair(COLOR_MATCHED,       shadow(base.matched,       theme.matched),       shadow(base.matched_bg, theme.matched_bg));
    init_pair(COLOR_CURRENT,       shadow(base.current,       theme.current),       shadow(base.current_bg, theme.current_bg));
    init_pair(COLOR_CURRENT_MATCH, shadow(base.current_match, theme.current_match), shadow(base.current_match_bg, theme.current_match_bg));
    init_pair(COLOR_SPINNER,       shadow(base.spinner,       theme.spinner),       *bg);
    init_pair(COLOR_INFO,          shadow(base.info,          theme.info),          *bg);
    init_pair(COLOR_CURSOR,        shadow(base.cursor,        theme.cursor),        shadow(base.current_bg, theme.current_bg));
    init_pair(COLOR_SELECTED,      shadow(base.selected,      theme.selected),      shadow(base.current_bg, theme.current_bg));
    init_pair(COLOR_HEADER,        shadow(base.header,        theme.header),        shadow(base.bg, theme.bg));
}


pub fn get_color_pair(fg: i16, bg: i16) -> attr_t {
    let fg = if fg == -1 { *FG.read().unwrap() } else {fg};
    let bg = if bg == -1 { *BG.read().unwrap() } else {bg};

    let key = (fg << 8) + bg;
    let mut color_map = COLOR_MAP.write().unwrap();
    let pair_num = color_map.len() as i16;
    let pair = color_map.entry(key).or_insert_with(|| {
        let next_pair = COLOR_USER + pair_num;
        init_pair(next_pair, fg, bg);
        COLOR_PAIR(next_pair)
    });
    *pair
}

#[derive(PartialEq, Eq, Clone, Debug, Copy)]
pub enum Margin {
    Fixed(i32),
    Percent(i32),
}

// A curse object is an abstraction of the screen to be draw on
// |
// |
// |
// +------------+ start_line
// |  ^         |
// | <          | <-- top = start_line + margin_top
// |  (margins) |
// |           >| <-- bottom = end_line - margin_bottom
// |          v |
// +------------+ end_line
// |
// |

pub struct Curses {
    //screen: SCREEN,
    term: Option<Box<Write>>,
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
    start_y: i32,
    height: Margin,
    margin_top: Margin,
    margin_bottom: Margin,
    margin_left: Margin,
    margin_right: Margin,
    rx_cursor_pos: Receiver<(u16, u16)>,
}

unsafe impl Send for Curses {}

impl Curses {
    pub fn new(options: &getopts::Matches, rx_cursor_pos: Receiver<(u16,u16)>) -> Self {
        // reserve enough lines according to height
        //

        let margins = if let Some(margin_option) = options.opt_str("margin") {
            Curses::parse_margin(&margin_option)
        } else {
            (Margin::Fixed(0), Margin::Fixed(0), Margin::Fixed(0), Margin::Fixed(0))
        };
        let (margin_top, margin_right, margin_bottom, margin_left) = margins;

        let height = if let Some(height_option) = options.opt_str("height") {
            Curses::parse_margin_string(&height_option)
        } else {
            Margin::Percent(100)
        };

        let (y, _) = Curses::get_cursor_pos();

        // reserve the necessary lines to show skim
        let (max_y, _) = Curses::terminal_size();
        Curses::reserve_lines(y, max_y, height);

        let start_y = match height {
            Margin::Percent(100) => 0,
            Margin::Percent(p) => min(y, max_y- p*max_y/100),
            Margin::Fixed(rows) => min(y, max_y - rows),
        };

        let term: Box<Write> = if Margin::Percent(100) == height {
            Box::new(AlternateScreen::from(stdout().into_raw_mode().unwrap()))
        } else {
            Box::new(stdout().into_raw_mode().unwrap())
        };

        let mut ret = Curses {
            term: Some(term),
            top: 0,
            bottom: 0,
            left: 0,
            right: 0,
            start_y,
            height,
            margin_top,
            margin_bottom,
            margin_left,
            margin_right,
            rx_cursor_pos,
        };
        ret.resize();
        ret
    }

    fn reserve_lines(y: i32, max_y: i32, height: Margin) {
        let rows = match height {
            Margin::Percent(100) => {return;}
            Margin::Percent(percent) => max_y*percent/100,
            Margin::Fixed(rows) => rows,
        };

        print!("{}", "\n".repeat((rows-1) as usize));
        stdout().flush().unwrap();
    }

    fn get_cursor_pos() -> (i32, i32) {
        let mut stdout = stdout().into_raw_mode().unwrap();
        let mut f = stdin();
        write!(stdout, "\x1B[6n").unwrap();
        stdout.flush().unwrap();

        let mut read_chars = Vec::new();
        loop {
            let mut buf = [0; 1];
            let _ = f.read(&mut buf);
            read_chars.push(buf[0]);
            if buf[0] == b'R' {
                break;
            }
        }
        let s = String::from_utf8(read_chars).unwrap();
        let t: Vec<&str> = s[2..s.len()-1].split(';').collect();
        stdout.flush().unwrap();
        (t[0].parse::<i32>().unwrap() - 1, t[1].parse::<i32>().unwrap() - 1)
    }

    fn parse_margin_string(margin: &str) -> Margin {
        if margin.ends_with("%") {
            Margin::Percent(min(100, margin[0..margin.len()-1].parse::<i32>().unwrap_or(100)))
        } else {
            Margin::Fixed(margin.parse::<i32>().unwrap_or(0))
        }
    }

    pub fn parse_margin(margin_option: &str) -> (Margin, Margin, Margin, Margin) {
        let margins = margin_option
            .split(",")
            .collect::<Vec<&str>>();

        match margins.len() {
            1 => {
                let margin = Curses::parse_margin_string(margins[0]);
                (margin, margin, margin, margin)
            }
            2 => {
                let margin_tb = Curses::parse_margin_string(margins[0]);
                let margin_rl = Curses::parse_margin_string(margins[1]);
                (margin_tb, margin_rl, margin_tb, margin_rl)
            }
            3 => {
                let margin_top = Curses::parse_margin_string(margins[0]);
                let margin_rl = Curses::parse_margin_string(margins[1]);
                let margin_bottom = Curses::parse_margin_string(margins[2]);
                (margin_top, margin_rl, margin_bottom, margin_rl)
            }
            4 => {
                let margin_top = Curses::parse_margin_string(margins[0]);
                let margin_right = Curses::parse_margin_string(margins[1]);
                let margin_bottom = Curses::parse_margin_string(margins[2]);
                let margin_left = Curses::parse_margin_string(margins[3]);
                (margin_top, margin_right, margin_bottom, margin_left)
            }
            _ => (Margin::Fixed(0), Margin::Fixed(0), Margin::Fixed(0), Margin::Fixed(0))
        }
    }

    fn get_color(&self, pair: i16, is_bold: bool) -> attr_t {
        if *USE_COLOR.read().unwrap() {
            attr_color(pair, is_bold)
        } else {
            attr_mono(pair, is_bold)
        }
    }

    pub fn resize(&mut self) {
        let (max_y, max_x) = Curses::terminal_size();
        let height = match self.height {
            Margin::Percent(100) => max_y,
            Margin::Percent(p) => min(max_y, p*max_y/100),
            Margin::Fixed(rows) => min(max_y, rows),
        };

        let start = if self.height == Margin::Percent(100) { 0 } else { self.start_y };

        self.top = start + match self.margin_top {
            Margin::Fixed(num) => num,
            Margin::Percent(per) => per * height / 100,
        };

        self.bottom = start + height - match self.margin_bottom {
            Margin::Fixed(num) => num,
            Margin::Percent(per) => per * height / 100,
        };

        self.left = match self.margin_left {
            Margin::Fixed(num) => num,
            Margin::Percent(per) => per * max_x / 100,
        };

        self.right = max_x - match self.margin_right {
            Margin::Fixed(num) => num,
            Margin::Percent(per) => per * max_x / 100,
        };
    }

    fn get_term(&mut self) -> &mut Box<Write> {
        self.term.as_mut().unwrap()
    }

    pub fn mv(&mut self, y: i32, x: i32) {
        //mv(y+self.top, x+self.left);
        let target_x = (x+self.left+1) as u16;
        let target_y = (y+self.top+1) as u16;
        write!(self.get_term(), "{}", termion::cursor::Goto(target_x, target_y));
    }

    pub fn get_maxyx(&self) -> (i32, i32) {
        let (max_y, max_x) = Curses::terminal_size();
        (self.bottom-self.top, self.right-self.left)
    }

    pub fn getyx(&mut self) -> (i32, i32) {
        write!(self.get_term(), "\x1B[6n");
        self.get_term().flush().unwrap();
        let (y, x) = self.rx_cursor_pos.recv().unwrap();
        (y as i32 - self.top, x as i32 - self.left)
    }

    fn terminal_size() -> (i32, i32) {
        let (max_x, max_y) = termion::terminal_size().unwrap();
        (max_y as i32, max_x as i32)
    }

    pub fn clrtoeol(&mut self) {
        write!(self.get_term(), "{}", termion::clear::UntilNewline);
    }

    pub fn endwin(&self) {
        //endwin();
    }

    fn height(&self) -> i32 {
        let (max_y, _) = Curses::terminal_size();
        match self.height {
            Margin::Percent(100) => max_y,
            Margin::Percent(p) => min(max_y, p*max_y/100),
            Margin::Fixed(rows) => min(max_y, rows),
        }
    }

    pub fn erase(&mut self) {
        //erase();
        println_stderr!("erase");
        for row in (0..self.height()).rev() {
            self.mv(row, 0);
            write!(self.get_term(), "{}", termion::clear::CurrentLine);
        }
    }

    pub fn cprint(&mut self, text: &str, pair: i16, is_bold: bool) {
        //let attr = self.get_color(pair, is_bold);
        //attron(attr);
        //addstr(text);
        //attroff(attr);
        println_stderr!("cprint: {}", text);
        write!(self.get_term(), "{}", text);
    }

    pub fn caddch(&mut self, ch: char, pair: i16, is_bold: bool) {
        //let attr = self.get_color(pair, is_bold);
        //attron(attr);
        //addstr(&ch.to_string()); // to support wide character
        //attroff(attr);
        println_stderr!("caddch: {}", ch);
        write!(self.get_term(), "{}", ch);
    }

    pub fn printw(&mut self, text: &str) {
        //printw(text);
        println_stderr!("printw: {}", text);
        write!(self.get_term(), "{}", text);
    }

    pub fn close(&mut self) {
        //endwin();
        //delscreen(self.screen);
        self.erase();
        self.term.take();
    }

    pub fn attr_on(&self, attr: attr_t) {
        //if attr == 0 {
            //attrset(0);
        //} else {
            //attron(attr);
        //}
    }

    pub fn refresh(&mut self) {
        //refresh();
        println_stderr!("refresh");
        self.get_term().flush().unwrap();
    }
}

// use default if x is COLOR_UNDEFINED, else use x
fn shadow(default: i16, x: i16) -> i16 {
    if x == COLOR_UNDEFINED { default } else { x }
}


fn attr_color(pair: i16, is_bold: bool) -> attr_t {
    let attr = if pair > COLOR_NORMAL {COLOR_PAIR(pair)} else {0};

    attr | if is_bold {A_BOLD()} else {0}
}

fn attr_mono(pair: i16, is_bold: bool) -> attr_t {
    let mut attr = 0;
    match pair {
        x if x == COLOR_NORMAL => {
            if is_bold {
                attr = A_REVERSE();
            }
        }
        x if x == COLOR_MATCHED => {
            attr = A_UNDERLINE();
        }
        x if x == COLOR_CURRENT_MATCH => {
            attr = A_UNDERLINE() | A_REVERSE()
        }
        _ => {}
    }
    attr | if is_bold {A_BOLD()} else {0}
}

const COLOR_DEFAULT: i16 = -1;
const COLOR_UNDEFINED: i16 = -2;

#[derive(Clone, Debug)]
pub struct ColorTheme {
    use_default: bool,

    fg: i16, // text fg
    bg: i16, // text bg
    matched: i16,
    matched_bg: i16,
    current: i16,
    current_bg: i16,
    current_match: i16,
    current_match_bg: i16,
    spinner: i16,
    info: i16,
    prompt: i16,
    cursor: i16,
    selected: i16,
    header: i16,
}

impl ColorTheme {
    pub fn new() -> Self {
        ColorTheme {
            use_default:  true,
            fg:               COLOR_UNDEFINED,
            bg:               COLOR_UNDEFINED,
            matched:          COLOR_UNDEFINED,
            matched_bg:       COLOR_UNDEFINED,
            current:          COLOR_UNDEFINED,
            current_bg:       COLOR_UNDEFINED,
            current_match:    COLOR_UNDEFINED,
            current_match_bg: COLOR_UNDEFINED,
            spinner:          COLOR_UNDEFINED,
            info:             COLOR_UNDEFINED,
            prompt:           COLOR_UNDEFINED,
            cursor:           COLOR_UNDEFINED,
            selected:         COLOR_UNDEFINED,
            header:           COLOR_UNDEFINED,
        }
    }

    pub fn from_options(color: &str) -> Self {
        let mut theme = ColorTheme::new();
        for pair in color.split(',') {
            let color: Vec<&str> = pair.split(':').collect();
            if color.len() < 2 {
                theme = match color[0] {
                    "molokai" => MONOKAI256.clone(),
                    "light" => LIGHT256.clone(),
                    "16"  => DEFAULT16.clone(),
                    "dark" | _ => DARK256.clone(),
                }
            }

            match color[0] {
                "fg"               => theme.fg = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "bg"               => theme.bg = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "matched"          => theme.matched = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "matched_bg"       => theme.matched_bg = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "current"          => theme.current = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "current_bg"       => theme.current_bg = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "current_match"    => theme.current_match = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "current_match_bg" => theme.current_match_bg = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "spinner"          => theme.spinner = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "info"             => theme.info = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "prompt"           => theme.prompt = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "cursor"           => theme.cursor = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "selected"         => theme.selected = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                "header"           => theme.header = color[1].parse().unwrap_or(COLOR_UNDEFINED),
                _ => {}
            }
        }
        theme
    }
}

const DEFAULT16: ColorTheme = ColorTheme {
    use_default:   true,
    fg:               15,
    bg:               0,
    matched:          COLOR_GREEN,
    matched_bg:       COLOR_BLACK,
    current:          COLOR_YELLOW,
    current_bg:       COLOR_BLACK,
    current_match:    COLOR_GREEN,
    current_match_bg: COLOR_BLACK,
    spinner:          COLOR_GREEN,
    info:             COLOR_WHITE,
    prompt:           COLOR_BLUE,
    cursor:           COLOR_RED,
    selected:         COLOR_MAGENTA,
    header:           COLOR_CYAN,
};

const DARK256: ColorTheme = ColorTheme {
    use_default:   true,
    fg:               15,
    bg:               0,
    matched:          108,
    matched_bg:       0,
    current:          254,
    current_bg:       236,
    current_match:    151,
    current_match_bg: 236,
    spinner:          148,
    info:             144,
    prompt:           110,
    cursor:           161,
    selected:         168,
    header:           109,
};

const MONOKAI256: ColorTheme = ColorTheme {
    use_default:   true,
    fg:               252,
    bg:               234,
    matched:          234,
    matched_bg:       186,
    current:          254,
    current_bg:       236,
    current_match:    234,
    current_match_bg: 186,
    spinner:          148,
    info:             144,
    prompt:           110,
    cursor:           161,
    selected:         168,
    header:           109,
};

const LIGHT256: ColorTheme = ColorTheme {
    use_default:   true,
    fg:               15,
    bg:               0,
    matched:          0,
    matched_bg:       220,
    current:          237,
    current_bg:       251,
    current_match:    66,
    current_match_bg: 251,
    spinner:          65,
    info:             101,
    prompt:           25,
    cursor:           161,
    selected:         168,
    header:           31,
};

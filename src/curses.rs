// An abstract layer towards ncurses-rs, which provides keycode, color scheme support
// Modeled after fzf

//use ncurses::*;
use getopts;
use std::sync::RwLock;
use std::collections::HashMap;
use std::io::{stdin, stdout, Write};
use std::io::prelude::*;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;
use termion;
use std::cmp::min;
use termion::color;
use std::fmt;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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

pub type attr_t = i16;

lazy_static! {
    // all colors are refered by the pair number
    static ref RESOURCE_MAP: RwLock<HashMap<attr_t, String>> = RwLock::new(HashMap::new());

    // COLOR_MAP is used to store ((fg <<8 ) | bg) -> attr_t, used to handle ANSI code
    static ref COLOR_MAP: RwLock<HashMap<String, attr_t>> = RwLock::new(HashMap::new());
}

// register the color as color pair
fn register_resource(key: attr_t, resource: String) {
    let mut resource_map = RESOURCE_MAP.write().unwrap();
    resource_map.entry(key).or_insert_with(|| {resource});
}

pub fn register_ansi(ansi: String) -> attr_t {
    //let fg = if fg == -1 { *FG.read().unwrap() } else {fg};
    //let bg = if bg == -1 { *BG.read().unwrap() } else {bg};

    let mut color_map = COLOR_MAP.write().unwrap();
    let pair_num = color_map.len() as i16;
    if color_map.contains_key(&ansi) {
        *color_map.get(&ansi).unwrap()
    } else {
        let next_pair = COLOR_USER + pair_num;
        register_resource(next_pair, ansi.clone());
        color_map.insert(ansi, next_pair);
        next_pair
    }
}

//pub fn init(theme: Option<&ColorTheme>, is_black: bool, _use_mouse: bool) {
    //return;
    //// initialize ncurses
    //let mut use_color = USE_COLOR.write().unwrap();

    //if let Some(theme) = theme {
        //let base_theme = if tigetnum("colors") >= 256 {
            //DARK256
        //} else {
            //DEFAULT16
        //};

        //init_pairs(&base_theme, theme, is_black);
        //*use_color = true;
    //} else {
        //*use_color = false;
    //}
//}

//fn init_pairs(base: &ColorTheme, theme: &ColorTheme, is_black: bool) {
    //let mut fg = FG.write().unwrap();
    //let mut bg = BG.write().unwrap();


    //*fg = shadow(base.fg, theme.fg);
    //*bg = shadow(base.bg, theme.bg);

    //if is_black {
        //*bg = COLOR_BLACK;
    //} else if theme.use_default {
        //*fg = COLOR_DEFAULT;
        //*bg = COLOR_DEFAULT;
        //use_default_colors();
    //}

    //if !theme.use_default {
        //assume_default_colors(shadow(base.fg, theme.fg) as i32, shadow(base.bg, theme.bg) as i32);
    //}

    //start_color();

    //init_pair(COLOR_PROMPT,        shadow(base.prompt,        theme.prompt),        *bg);
    //init_pair(COLOR_MATCHED,       shadow(base.matched,       theme.matched),       shadow(base.matched_bg, theme.matched_bg));
    //init_pair(COLOR_CURRENT,       shadow(base.current,       theme.current),       shadow(base.current_bg, theme.current_bg));
    //init_pair(COLOR_CURRENT_MATCH, shadow(base.current_match, theme.current_match), shadow(base.current_match_bg, theme.current_match_bg));
    //init_pair(COLOR_SPINNER,       shadow(base.spinner,       theme.spinner),       *bg);
    //init_pair(COLOR_INFO,          shadow(base.info,          theme.info),          *bg);
    //init_pair(COLOR_CURSOR,        shadow(base.cursor,        theme.cursor),        shadow(base.current_bg, theme.current_bg));
    //init_pair(COLOR_SELECTED,      shadow(base.selected,      theme.selected),      shadow(base.current_bg, theme.current_bg));
    //init_pair(COLOR_HEADER,        shadow(base.header,        theme.header),        shadow(base.bg, theme.bg));
//}


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

pub struct Window {
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,

    wrap: bool,
    border: Option<Direction>,
    stdout_buffer: String,
    current_y: i32,
    current_x: i32,
}

impl Default for Window {
    fn default() -> Self {
        Window {
            top: 0,
            bottom: 0,
            left: 0,
            right: 0,
            wrap: false,
            border: None,
            stdout_buffer: String::new(),
            current_x: 0,
            current_y: 0,
        }
    }
}

impl Window {
    pub fn new(top: i32, right: i32, bottom: i32, left: i32, wrap: bool, border: Option<Direction>) -> Self {
        Window {
            top,
            bottom,
            left,
            right,
            border,
            wrap,
            stdout_buffer: String::with_capacity(CURSES_BUF_SIZE),
            current_x: 0,
            current_y: 0,
        }
    }

    pub fn reshape(&mut self, top: i32, right: i32, bottom: i32, left: i32) {
        self.top = top;
        self.right = right;
        self.bottom = bottom;
        self.left = left;
    }

    pub fn set_border(&mut self, border: Option<Direction>) {
        self.border = border;
    }

    pub fn draw_border(&mut self) {
        debug!("curses:window:draw_border: TRBL: {}, {}, {}, {}", self.top, self.right, self.bottom, self.left);
        let (y, x) = self.getyx();
        match self.border {
            Some(Direction::Up) => {
                self.stdout_buffer.push_str(format!("{}", termion::cursor::Goto(self.left as u16 +1, self.top as u16 +1)).as_str());
                self.stdout_buffer.push_str(&"-".repeat((self.right-self.left) as usize));
            }
            Some(Direction::Down) => {
                self.stdout_buffer.push_str(format!("{}", termion::cursor::Goto(self.left as u16 +1, self.bottom as u16)).as_str());
                self.stdout_buffer.push_str(&"-".repeat((self.right-self.left) as usize));
            }
            Some(Direction::Left) => {
                for i in self.top..self.bottom {
                    self.stdout_buffer.push_str(format!("{}", termion::cursor::Goto(self.left as u16 +1, i as u16+1)).as_str());
                    self.stdout_buffer.push_str("|");
                }
            }
            Some(Direction::Right) => {
                for i in self.top..self.bottom {
                    self.stdout_buffer.push_str(format!("{}", termion::cursor::Goto(self.right as u16, i as u16+1)).as_str());
                    self.stdout_buffer.push_str("|");
                }
            }
            _ => {}
        }
        self.mv(y, x);
    }

    pub fn mv(&mut self, y: i32, x: i32) {
        self.current_y = y;
        self.current_x = x;
        let (target_y, target_x) = match self.border {
            Some(Direction::Up)    => ((y+self.top+1+1) as u16, (x+self.left+1)   as u16),
            Some(Direction::Down)  => ((y+self.top+1)   as u16, (x+self.left+1)   as u16),
            Some(Direction::Left)  => ((y+self.top+1)   as u16, (x+self.left+1+1) as u16),
            Some(Direction::Right) => ((y+self.top+1)   as u16, (x+self.left+1)   as u16),
            _                      => ((y+self.top+1)   as u16, (x+self.left+1)   as u16),
        };
        self.stdout_buffer.push_str(format!("{}", termion::cursor::Goto(target_x, target_y)).as_str());
    }

    pub fn get_maxyx(&self) -> (i32, i32) {
        let (max_y, max_x) = (self.bottom-self.top, self.right-self.left);

        match self.border {
            Some(Direction::Up) | Some(Direction::Down) => (max_y-1, max_x),
            Some(Direction::Left) | Some(Direction::Right) => (max_y, max_x-1),
            _ => (max_y, max_x),
        }
    }

    pub fn getyx(&mut self) -> (i32, i32) {
        (self.current_y, self.current_x)
    }

    pub fn clrtoeol(&mut self) {
        let (y, x) = self.getyx();
        let (max_y, max_x) = self.get_maxyx();
        if y >= max_y || x >= max_x {
            return;
        }

        self.stdout_buffer.push_str(&" ".repeat((max_x - x) as usize));
        self.mv(y, x);
    }

    pub fn erase(&mut self) {
        let (max_y, _) = self.get_maxyx();
        for row in (0..max_y).rev() {
            self.mv(row, 0);
            self.clrtoeol();
        }
    }

    pub fn clrtoend(&mut self) {
        let (y, x) = self.getyx();
        let (max_y, max_x) = self.get_maxyx();

        debug!("curses:window:clrtoend: y/x: {}/{}, max_y/max_x: {}/{}", y, x, max_y, max_x);

        self.clrtoeol();
        for row in y+1..max_y {
            self.mv(row, 0);
            self.clrtoeol();
        }
    }

    pub fn printw(&mut self, text: &str) {
        debug!("curses:window:printw: {:?}", text);
        for ch in text.chars() {
            self.add_char(ch);
        }
    }

    pub fn cprint(&mut self, text: &str, pair: i16, is_bold: bool) {
        self.attron(pair);
        self.printw(text);
        self.attroff(pair);
    }

    pub fn caddch(&mut self, ch: char, pair: i16, is_bold: bool) {
        self.attron(pair);
        self.add_char(ch);
        self.attroff(pair);
    }

    pub fn addch(&mut self, ch: char) {
        self.add_char(ch);
    }

    fn add_char(&mut self, ch: char) {
        let (max_y, max_x) = self.get_maxyx();
        let (y, x) = self.getyx();
        if y >= max_y {
            return;
        }

        debug!("curses:window:add_char: {:?}", ch);

        match ch {
            '\t' => {
                let tabstop = 8;
                let rest = 8 - self.current_x % tabstop;
                for _ in 0..rest {
                    self.add_char_raw(' ');
                }
            }
            '\r' => {
                let (y, x) = self.getyx();
                self.mv(y, 0);
            }
            '\n' => {
                let (y, x) = self.getyx();
                self.clrtoeol();
                self.mv(y+1, 0);
            }
            ch => {
                self.add_char_raw(ch);
            }
        }
    }

    fn add_char_raw(&mut self, ch: char) {
        let (max_y, max_x) = self.get_maxyx();
        let (y, x) = self.getyx();
        let text_width = ch.width_cjk().unwrap_or(2) as i32;
        let target_x = x + text_width;


        // no enough space to print
        if (y >= max_y) || (target_x > max_x && y == max_y-1) || (!self.wrap && target_x > max_x) {
            return;
        }

        if target_x > max_x {
            self.mv(y+1, 0);
        }

        self.stdout_buffer.push(ch);

        let (y, x) = self.getyx();
        let target_x = x + text_width;

        let final_x = if self.wrap {target_x % max_x} else {target_x};
        let final_y = y + if self.wrap {target_x/max_x} else {0};
        self.mv(final_y, final_x);
    }

    pub fn attr_on(&mut self, attr: attr_t) {
        if attr == 0 {
            self.attrclear();
        } else {
            self.attron(attr);
        }
    }

    fn attron(&mut self, key: attr_t) {
        let resource_map = RESOURCE_MAP.read().unwrap();
        resource_map.get(&key).map(|s| self.stdout_buffer.push_str(s));
    }

    fn attroff(&mut self, _: attr_t) {
        self.stdout_buffer.push_str(format!("{}{}", color::Fg(color::Reset), color::Bg(color::Reset)).as_str());
    }

    fn attrclear(&mut self) {
        self.stdout_buffer.push_str(format!("{}{}", color::Fg(color::Reset), color::Bg(color::Reset)).as_str());
    }

    pub fn write_to_term(&mut self, term: &mut Write) {
        write!(term, "{}", &self.stdout_buffer).unwrap();
        self.stdout_buffer.clear();
    }

    pub fn hide_cursor(&mut self) {
        self.stdout_buffer.push_str(format!("{}", termion::cursor::Hide).as_str());
    }
    pub fn show_cursor(&mut self) {
        self.stdout_buffer.push_str(format!("{}", termion::cursor::Show).as_str());
    }
}


#[derive(PartialEq, Eq, Clone, Debug, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}


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

    current_y: i32,
    current_x: i32,

    // preview window status
    preview_direction: Direction,
    preview_size: Margin,
    preview_shown: bool,

    pub win_main: Window,
    pub win_preview: Window,
}

unsafe impl Send for Curses {}

const CURSES_BUF_SIZE: usize = 100 * 1024;

impl Curses {
    pub fn new(options: &getopts::Matches) -> Self {
        ColorTheme::init_from_options(&options);

        // reserve enough lines according to height

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
        Curses::reserve_lines(max_y, height);

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

        // parse options for preview window
        let (preview_direction, preview_size, preview_shown) = if let Some(opts) = options.opt_str("preview-window") {
            Curses::parse_preview(&opts)
        } else {
            (Direction::Right, Margin::Percent(50), true)
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
            current_y: 0,
            current_x: 0,

            preview_direction,
            preview_size,
            preview_shown,

            win_main: Window::new(0,0,0,0, false, None),
            win_preview: Window::new(0,0,0,0, true, None),
        };
        ret.resize();
        ret
    }

    fn reserve_lines(max_y: i32, height: Margin) {
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

    // -> (direction, size, shown)
    fn parse_preview(preview_option: &str) -> (Direction, Margin, bool) {
        let options = preview_option
            .split(":")
            .collect::<Vec<&str>>();

        let direction = if let Some(ref direction) = options.get(0) {
            match direction.to_uppercase().as_str() {
                "UP"    => Direction::Up,
                "DOWN"  => Direction::Down,
                "LEFT"  => Direction::Left,
                "RIGHT" => Direction::Right,
                _       => Direction::Right,
            }
        } else {
            Direction::Right
        };

        let size = if let Some(ref size) = options.get(1) {
            Curses::parse_margin_string(size)
        } else {
            Margin::Percent(50)
        };

        let shown = if let Some(ref direction) = options.get(2) {
            match direction.to_uppercase().as_str() {
                "HIDDEN" => false,
                _        => true,
            }
        } else {
            true
        };

        (direction, size, shown)
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

        debug!("curses:resize, TRBL: {}/{}/{}/{}", self.top, self.right, self.bottom, self.left);

        let height = self.bottom - self.top;
        let width = self.right - self.left;

        let preview_height = match self.preview_size {
            Margin::Fixed(x) => x,
            Margin::Percent(x) => height * x / 100,
        };

        let preview_width = match self.preview_size {
            Margin::Fixed(x) => x,
            Margin::Percent(x) => width * x / 100,
        };


        if !self.preview_shown {
            self.win_main.reshape(self.top, self.right, self.bottom, self.left);
            self.win_preview.reshape(0, 0, 0, 0);
        } else {
            match self.preview_direction {
                Direction::Up => {
                    self.win_preview.reshape(self.top, self.right, self.top+preview_height, self.left);
                    self.win_main.reshape(self.top+preview_height, self.right, self.bottom, self.left);
                    self.win_preview.set_border(Some(Direction::Down));
                }
                Direction::Down => {
                    self.win_preview.reshape(self.bottom-preview_height, self.right, self.bottom, self.left);
                    self.win_main.reshape(self.top, self.right, self.bottom-preview_height, self.left);
                    self.win_preview.set_border(Some(Direction::Up));
                }
                Direction::Left => {
                    self.win_preview.reshape(self.top, self.left+preview_width, self.bottom, self.left);
                    self.win_main.reshape(self.top, self.right, self.bottom, self.left+preview_width);
                    self.win_preview.set_border(Some(Direction::Right));
                }
                Direction::Right => {
                    self.win_preview.reshape(self.top, self.right, self.bottom, self.right-preview_width);
                    self.win_main.reshape(self.top, self.right-preview_width, self.bottom, self.left);
                    self.win_preview.set_border(Some(Direction::Left));
                }
            }
        }
    }

    fn terminal_size() -> (i32, i32) {
        let (max_x, max_y) = termion::terminal_size().unwrap();
        (max_y as i32, max_x as i32)
    }

    fn height(&self) -> i32 {
        let (max_y, _) = Curses::terminal_size();
        match self.height {
            Margin::Percent(100) => max_y,
            Margin::Percent(p) => min(max_y, p*max_y/100),
            Margin::Fixed(rows) => min(max_y, rows),
        }
    }

    pub fn close(&mut self) {
        self.win_preview.erase();
        self.win_main.erase();
        self.refresh();
        self.term.take();
    }

    pub fn refresh(&mut self) {
        let mut term = self.term.as_mut().unwrap();
        self.win_preview.write_to_term(term);
        self.win_main.write_to_term(term);
        term.flush().unwrap();
    }
}

//fn attr_color(pair: i16, is_bold: bool) -> attr_t {
    //let attr = if pair > COLOR_NORMAL {COLOR_PAIR(pair)} else {0};

    //attr | if is_bold {A_BOLD()} else {0}
//}

//fn attr_mono(pair: i16, is_bold: bool) -> attr_t {
    //let mut attr = 0;
    //match pair {
        //x if x == COLOR_NORMAL => {
            //if is_bold {
                //attr = A_REVERSE();
            //}
        //}
        //x if x == COLOR_MATCHED => {
            //attr = A_UNDERLINE();
        //}
        //x if x == COLOR_CURRENT_MATCH => {
            //attr = A_UNDERLINE() | A_REVERSE()
        //}
        //_ => {}
    //}
    //attr | if is_bold {A_BOLD()} else {0}
//}

struct ColorWrapper(Box<color::Color>);

impl color::Color for ColorWrapper {
    fn write_fg(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.write_fg(f)
    }
    fn write_bg(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.write_bg(f)
    }
}

impl<'a> color::Color for &'a ColorWrapper {
    fn write_fg(&self, f: &mut fmt::Formatter) -> fmt::Result {
        (*self).write_fg(f)
    }
    fn write_bg(&self, f: &mut fmt::Formatter) -> fmt::Result {
        (*self).write_bg(f)
    }
}

pub struct ColorTheme {
    fg:               ColorWrapper, // text fg
    bg:               ColorWrapper, // text bg
    matched:          ColorWrapper,
    matched_bg:       ColorWrapper,
    current:          ColorWrapper,
    current_bg:       ColorWrapper,
    current_match:    ColorWrapper,
    current_match_bg: ColorWrapper,
    spinner:          ColorWrapper,
    info:             ColorWrapper,
    prompt:           ColorWrapper,
    cursor:           ColorWrapper,
    selected:         ColorWrapper,
    header:           ColorWrapper,
}


impl ColorTheme {
    pub fn init_from_options(options: &getopts::Matches) {
        // register
        let theme = if let Some(color) = options.opt_str("color") {
            ColorTheme::from_options(&color)
        } else {
            ColorTheme::dark256()
        };
        theme.register_self();
    }

    fn default() -> Self {
        ColorTheme {
            fg:               ColorWrapper(Box::new(color::Reset)),
            bg:               ColorWrapper(Box::new(color::Reset)),
            matched:          ColorWrapper(Box::new(color::Green)),
            matched_bg:       ColorWrapper(Box::new(color::Black)),
            current:          ColorWrapper(Box::new(color::Yellow)),
            current_bg:       ColorWrapper(Box::new(color::Black)),
            current_match:    ColorWrapper(Box::new(color::Green)),
            current_match_bg: ColorWrapper(Box::new(color::Black)),
            spinner:          ColorWrapper(Box::new(color::Green)),
            info:             ColorWrapper(Box::new(color::White)),
            prompt:           ColorWrapper(Box::new(color::Blue)),
            cursor:           ColorWrapper(Box::new(color::Red)),
            selected:         ColorWrapper(Box::new(color::Magenta)),
            header:           ColorWrapper(Box::new(color::Cyan)),
        }
    }

    fn dark256() -> Self {
        ColorTheme {
            fg:               ColorWrapper(Box::new(color::Reset)),
            bg:               ColorWrapper(Box::new(color::Reset)),
            matched:          ColorWrapper(Box::new(color::AnsiValue(108))),
            matched_bg:       ColorWrapper(Box::new(color::AnsiValue(0))),
            current:          ColorWrapper(Box::new(color::AnsiValue(254))),
            current_bg:       ColorWrapper(Box::new(color::AnsiValue(236))),
            current_match:    ColorWrapper(Box::new(color::AnsiValue(151))),
            current_match_bg: ColorWrapper(Box::new(color::AnsiValue(236))),
            spinner:          ColorWrapper(Box::new(color::AnsiValue(148))),
            info:             ColorWrapper(Box::new(color::AnsiValue(144))),
            prompt:           ColorWrapper(Box::new(color::AnsiValue(110))),
            cursor:           ColorWrapper(Box::new(color::AnsiValue(161))),
            selected:         ColorWrapper(Box::new(color::AnsiValue(168))),
            header:           ColorWrapper(Box::new(color::AnsiValue(109))),
        }
    }

    fn monokai256() -> Self {
        ColorTheme {
            fg:               ColorWrapper(Box::new(color::AnsiValue(252))),
            bg:               ColorWrapper(Box::new(color::AnsiValue(234))),
            matched:          ColorWrapper(Box::new(color::AnsiValue(234))),
            matched_bg:       ColorWrapper(Box::new(color::AnsiValue(186))),
            current:          ColorWrapper(Box::new(color::AnsiValue(254))),
            current_bg:       ColorWrapper(Box::new(color::AnsiValue(236))),
            current_match:    ColorWrapper(Box::new(color::AnsiValue(234))),
            current_match_bg: ColorWrapper(Box::new(color::AnsiValue(186))),
            spinner:          ColorWrapper(Box::new(color::AnsiValue(148))),
            info:             ColorWrapper(Box::new(color::AnsiValue(144))),
            prompt:           ColorWrapper(Box::new(color::AnsiValue(110))),
            cursor:           ColorWrapper(Box::new(color::AnsiValue(161))),
            selected:         ColorWrapper(Box::new(color::AnsiValue(168))),
            header:           ColorWrapper(Box::new(color::AnsiValue(109))),
        }
    }

    fn light256() -> Self {
        ColorTheme {
            fg:               ColorWrapper(Box::new(color::Reset)),
            bg:               ColorWrapper(Box::new(color::Reset)),
            matched:          ColorWrapper(Box::new(color::AnsiValue(0))),
            matched_bg:       ColorWrapper(Box::new(color::AnsiValue(220))),
            current:          ColorWrapper(Box::new(color::AnsiValue(237))),
            current_bg:       ColorWrapper(Box::new(color::AnsiValue(251))),
            current_match:    ColorWrapper(Box::new(color::AnsiValue(66))),
            current_match_bg: ColorWrapper(Box::new(color::AnsiValue(251))),
            spinner:          ColorWrapper(Box::new(color::AnsiValue(65))),
            info:             ColorWrapper(Box::new(color::AnsiValue(101))),
            prompt:           ColorWrapper(Box::new(color::AnsiValue(25))),
            cursor:           ColorWrapper(Box::new(color::AnsiValue(161))),
            selected:         ColorWrapper(Box::new(color::AnsiValue(168))),
            header:           ColorWrapper(Box::new(color::AnsiValue(31))),
        }
    }

    fn from_options(color: &str) -> Self {
        let mut theme = ColorTheme::default();
        for pair in color.split(',') {
            let color: Vec<&str> = pair.split(':').collect();
            if color.len() < 2 {
                theme = match color[0] {
                    "molokai"  => ColorTheme::monokai256(),
                    "light"    => ColorTheme::light256(),
                    "16"       => ColorTheme::default(),
                    "dark" | _ => ColorTheme::dark256(),
                };
                continue;
            }

            let new_color = if color[1].len() == 7 {
                // 256 color
                let r = u8::from_str_radix(&color[1][1..3], 16).unwrap_or(255);
                let g = u8::from_str_radix(&color[1][3..5], 16).unwrap_or(255);
                let b = u8::from_str_radix(&color[1][5..7], 16).unwrap_or(255);
                ColorWrapper(Box::new(color::Rgb(r, g, b)))
            } else {
                ColorWrapper(Box::new(color::AnsiValue(color[1].parse::<u8>().unwrap_or(255))))
            };

            match color[0] {
                "fg"               => theme.fg = new_color,
                "bg"               => theme.bg = new_color,
                "matched"          => theme.matched = new_color,
                "matched_bg"       => theme.matched_bg = new_color,
                "current"          => theme.current = new_color,
                "current_bg"       => theme.current_bg = new_color,
                "current_match"    => theme.current_match = new_color,
                "current_match_bg" => theme.current_match_bg = new_color,
                "spinner"          => theme.spinner = new_color,
                "info"             => theme.info = new_color,
                "prompt"           => theme.prompt = new_color,
                "cursor"           => theme.cursor = new_color,
                "selected"         => theme.selected = new_color,
                "header"           => theme.header = new_color,
                _ => {}
            }
        }
        theme
    }

    fn register_self(&self) {
        register_resource(COLOR_NORMAL,        String::new());
        register_resource(COLOR_PROMPT,        format!("{}{}", color::Fg(&self.prompt),        color::Bg(&self.bg)));
        register_resource(COLOR_MATCHED,       format!("{}{}", color::Fg(&self.matched),       color::Bg(&self.matched_bg)));
        register_resource(COLOR_CURRENT,       format!("{}{}", color::Fg(&self.current),       color::Bg(&self.current_bg)));
        register_resource(COLOR_CURRENT_MATCH, format!("{}{}", color::Fg(&self.current_match), color::Bg(&self.current_match_bg)));
        register_resource(COLOR_SPINNER,       format!("{}{}", color::Fg(&self.spinner),       color::Bg(&self.bg)));
        register_resource(COLOR_INFO,          format!("{}{}", color::Fg(&self.info),          color::Bg(&self.bg)));
        register_resource(COLOR_CURSOR,        format!("{}{}", color::Fg(&self.cursor),        color::Bg(&self.current_bg)));
        register_resource(COLOR_SELECTED,      format!("{}{}", color::Fg(&self.selected),      color::Bg(&self.current_bg)));
        register_resource(COLOR_HEADER,        format!("{}{}", color::Fg(&self.header),        color::Bg(&self.bg)));
    }
}


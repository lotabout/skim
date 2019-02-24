// An abstract layer towards ncurses-rs, which provides keycode, color scheme support
// Modeled after fzf

//use ncurses::*;
use crate::options::SkimOptions;
use std::cmp::min;
use unicode_width::UnicodeWidthChar;
use std::sync::Arc;
use tuikit::term::Term;
use tuikit::attr::Attr;
use tuikit::screen::{Screen, Cell};
use crate::theme::{ColorTheme, DEFAULT_THEME};

//==============================================================================
const MIN_HEIGHT: usize = 3;
const MIN_WIDTH: usize = 4;

#[derive(PartialEq, Eq, Clone, Debug, Copy)]
pub enum Margin {
    Fixed(usize),
    Percent(usize),
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
// row `bottom` and column `right` should not be used.

pub struct Window {
    top: usize,
    bottom: usize,
    left: usize,
    right: usize,

    wrap: bool,
    border: Option<Direction>,

    current_y: usize,
    current_x: usize,
    screen: Screen,
    pub theme: ColorTheme,
}

pub struct WindowOption {
    pub top: usize,
    pub bottom: usize,
    pub left: usize,
    pub right: usize,
    pub wrap: bool,
    pub border: Option<Direction>,
    pub theme: ColorTheme,
}

impl Default for WindowOption {
    fn default() -> Self {
        Self {
            top: 0,
            bottom: 0,
            left: 0,
            right: 0,
            wrap: false,
            border: None,
            theme: DEFAULT_THEME,
        }
    }
}

impl Window {
    pub fn new(option: WindowOption) -> Self {
        let (width, height) = Self::calc_size(&option.border, option.top, option.right, option.bottom, option.left);

        Self {
            top: option.top,
            bottom: option.bottom,
            left: option.left,
            right: option.right,

            wrap: option.wrap,
            border: option.border,

            current_y: 0,
            current_x: 0,
            screen: Screen::new(width, height),
            theme: option.theme,
        }
    }

    fn calc_size(border: &Option<Direction>, top: usize, right: usize, bottom: usize, left: usize) -> (usize, usize) {
        match *border {
            Some(Direction::Up) | Some(Direction::Down) => (right - left, bottom - top - 1),
            Some(Direction::Left) | Some(Direction::Right) => (right - left-1, bottom - top),
            None => (right - left, bottom - top),
        }
    }

    pub fn reshape(&mut self, top: usize, right: usize, bottom: usize, left: usize) {
//        debug!("window:reshape, TRBL: {}/{}/{}/{}", self.top, self.right, self.bottom, self.left);
        self.top = top;
        self.right = right;
        self.bottom = bottom;
        self.left = left;
        let (width, height) = Self::calc_size(&self.border, top, right, bottom, left);
        self.screen.resize(width, height);
    }

    pub fn set_border(&mut self, border: Option<Direction>) {
        self.border = border;
    }

    #[rustfmt::skip]
    pub fn mv(&mut self, y: usize, x: usize) {
        self.current_y = y;
        self.current_x = x;
    }

    pub fn get_maxyx(&self) -> (usize, usize) {
        (self.screen.height(), self.screen.width())
    }

    pub fn getyx(&self) -> (usize, usize) {
        (self.current_y, self.current_x)
    }

    pub fn clrtoeol(&mut self) {
        let (y, x) = self.getyx();
        let (max_y, max_x) = self.get_maxyx();
        if y >= max_y || x >= max_x {
            return;
        }

        self.screen.print(y, x, &" ".repeat(max_x - x), self.theme.normal());
    }

    pub fn clrtoend(&mut self) {
        let (y, _) = self.getyx();
        let (max_y, max_x) = self.get_maxyx();

        //debug!("curses:window:clrtoend: y/x: {}/{}, max_y/max_x: {}/{}", y, x, max_y, max_x);

        self.clrtoeol();
        for row in y + 1..max_y {
            self.screen.print(row, 0, &" ".repeat(max_x), self.theme.normal());
        }
    }

    pub fn print(&mut self, text: &str) {
        //debug!("curses:window:printw: {:?}", text);
        self.print_with_attr(text, self.theme.normal());
    }

    pub fn print_with_attr(&mut self, text: &str, attr: Attr) {
        for ch in text.chars() {
            self.add_char_with_attr(ch, attr);
        }
    }

    pub fn add_char(&mut self, ch: char) {
        self.add_char_with_attr(ch, self.theme.normal());
    }

    pub fn add_char_with_attr(&mut self, ch: char, attr: Attr) {
        self.add_char_inner(ch, attr);
    }

    fn add_char_inner(&mut self, ch: char, attr: Attr) {
        let (max_y, _) = self.get_maxyx();
        let (y, _) = self.getyx();
        if y >= max_y {
            return;
        }

        //debug!("curses:window:add_char: {:?}", ch);

        match ch {
            '\t' => {
                let tabstop = 8;
                let rest = 8 - self.current_x % tabstop;
                for _ in 0..rest {
                    self.add_char_raw(' ', attr);
                }
            }
            '\r' => {
                let (y, _) = self.getyx();
                self.mv(y, 0);
            }
            '\n' => {
                let (y, _) = self.getyx();
                self.clrtoeol();
                self.mv(y + 1, 0);
            }
            ch => {
                self.add_char_raw(ch, attr);
            }
        }
    }

    fn add_char_raw(&mut self, ch: char, attr: Attr) {
        let (max_y, max_x) = self.get_maxyx();
        let (y, x) = self.getyx();
        let text_width = ch.width().unwrap_or(2) as usize;
        let target_x = x + text_width;

        // no enough space to print
        if (y >= max_y) || (target_x > max_x && y == max_y - 1) || (!self.wrap && target_x > max_x) {
            return;
        }

        self.screen.put_cell(y, x, Cell {ch, attr});

        if target_x > max_x {
            self.mv(y + 1, 0);
        }

        let (y, x) = self.getyx();
        let target_x = x + text_width;

        let final_x = if self.wrap { target_x % max_x } else { target_x };
        let final_y = y + if self.wrap { target_x / max_x } else { 0 };
        self.mv(final_y, final_x);
    }

    pub fn write_to_term(&mut self, term: &Term) {
        self.draw_border(term);

        for (row, col, &cell) in self.screen.iter_cell() {
            let (y, x) = self.adjust_cursor_offset(row, col);
            let _ = term.put_cell(y, x, cell);
        }

        let (row, col) = self.adjust_cursor_offset(self.current_y, self.current_x);
        let _ = term.set_cursor(row, col);
    }

    fn adjust_cursor_offset(&self, y: usize, x: usize) -> (usize, usize) {
        let (row, col) = match self.border {
            Some(Direction::Up) => (y+1, x),
            Some(Direction::Left) => (y, x+1),
            _ => (y, x)
        };

        (self.top + row, self.left + col)
    }

    fn draw_border(&mut self, term: &Term) {
        debug!("curses:window:draw_border: TRBL: {}, {}, {}, {}", self.top, self.right, self.bottom, self.left);
        match self.border {
            Some(Direction::Up) => {
                let _ = term.print_with_attr(self.top,
                                     self.left,
                                     &"─".repeat(self.right - self.left),
                                     self.theme.border());
            }
            Some(Direction::Down) => {
                let _ = term.print_with_attr(self.bottom-1,
                                     self.left,
                                     &"─".repeat(self.right - self.left),
                                     self.theme.border());
            }
            Some(Direction::Left) => for i in self.top..self.bottom {
                let _ = term.print_with_attr(i,
                                     self.left,
                                     "│",
                                     self.theme.border());
            },
            Some(Direction::Right) => for i in self.top..self.bottom {
                let _ = term.print_with_attr(i,
                                     self.right-1,
                                     "│",
                                     self.theme.border());
            },
            _ => {}
        }
    }

    pub fn hide_cursor(&mut self) {
        self.screen.show_cursor(false);
    }
    pub fn show_cursor(&mut self) {
        self.screen.show_cursor(true);
    }

    pub fn move_cursor_right(&mut self, offset: usize) {
        let (_, max_x) = self.get_maxyx();
        self.current_x = min(self.current_x + offset, max_x);
        self.screen.set_cursor(self.current_y, self.current_x);
    }

    pub fn close(&mut self) {
        self.screen.clear();
        self.screen.set_cursor(0, 0);
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
    term: Arc<Term>,

    top: usize,
    bottom: usize,
    left: usize,
    right: usize,
    // +3 means 3 lines from top, -3 means 3 lines from bottom,
    margin_top: Margin,
    margin_bottom: Margin,
    margin_left: Margin,
    margin_right: Margin,

    // preview window status
    preview_direction: Direction,
    preview_size: Margin,
    preview_shown: bool,

    pub win_main: Window,
    pub win_preview: Window,

    pub theme: ColorTheme,
}

unsafe impl Send for Curses {}

impl Curses {
    pub fn new(term: Arc<Term>, options: &SkimOptions) -> Self {
        let margins = options
            .margin
            .map(Curses::parse_margin)
            .expect("option margin is should be specified (by default)");
        let (margin_top, margin_right, margin_bottom, margin_left) = margins;

        // parse options for preview window
        let preview_cmd_exist = options.preview != None;
        let (preview_direction, preview_size, preview_wrap, preview_shown) = options
            .preview_window
            .map(Curses::parse_preview)
            .expect("option 'preview-window' should be set (by default)");

        let mut ret = Curses {
            term,
            top: 0,
            bottom: 0,
            left: 0,
            right: 0,
            margin_top,
            margin_bottom,
            margin_left,
            margin_right,

            preview_direction,
            preview_size,
            preview_shown: preview_cmd_exist && preview_shown,

            win_main: Window::new(WindowOption::default()),
            win_preview: Window::new(WindowOption {wrap: preview_wrap, ..WindowOption::default()}),

            theme: ColorTheme::init_from_options(options),
        };

        ret.resize();
        ret
    }

    fn parse_margin_string(margin: &str) -> Margin {
        if margin.ends_with('%') {
            Margin::Percent(min(100, margin[0..margin.len() - 1].parse::<usize>().unwrap_or(100)))
        } else {
            Margin::Fixed(margin.parse::<usize>().unwrap_or(0))
        }
    }

    pub fn parse_margin(margin_option: &str) -> (Margin, Margin, Margin, Margin) {
        let margins = margin_option.split(',').collect::<Vec<&str>>();

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
            _ => (Margin::Fixed(0), Margin::Fixed(0), Margin::Fixed(0), Margin::Fixed(0)),
        }
    }

    // -> (direction, size, wrap, shown)
    fn parse_preview(preview_option: &str) -> (Direction, Margin, bool, bool) {
        let options = preview_option.split(':').collect::<Vec<&str>>();

        let mut direction = Direction::Right;
        let mut shown = true;
        let mut wrap = false;
        let mut size = Margin::Percent(50);

        for option in options {
            // mistake
            if option.is_empty() {
                continue;
            }

            let first_char = option.chars().next().unwrap_or('A');

            // raw string
            if first_char.is_digit(10) {
                size = Curses::parse_margin_string(option);
            } else {
                match option.to_uppercase().as_str() {
                    "UP" => direction = Direction::Up,
                    "DOWN" => direction = Direction::Down,
                    "LEFT" => direction = Direction::Left,
                    "RIGHT" => direction = Direction::Right,
                    "HIDDEN" => shown = false,
                    "WRAP" => wrap = true,
                    _ => {}
                }
            }
        }

        (direction, size, wrap, shown)
    }

    fn margin_to_fixed(margin: &Margin, actual: usize) -> usize {
        match *margin {
            Margin::Fixed(num) => num,
            Margin::Percent(per) => per * actual / 100,
        }
    }

    #[rustfmt::skip]
    pub fn resize(&mut self) {
        let (term_width, term_height) = self.term.term_size().expect("failed to get terminal size");

//        debug!("term size: width/height ({}/{})", term_width, term_height);

        if term_width < MIN_WIDTH || term_height < MIN_HEIGHT {
            panic!("terminal is two small with width: {}, height: {}", term_width, term_height);
        }

//        debug!("margin, {:?}/{:?}/{:?}/{:?}", self.margin_top, self.margin_right, self.margin_bottom, self.margin_left);

        self.top = Self::margin_to_fixed(&self.margin_top, term_height);
        self.bottom = term_height - Self::margin_to_fixed(&self.margin_bottom, term_height);
        self.left = Self::margin_to_fixed(&self.margin_left, term_width);
        self.right = term_width - Self::margin_to_fixed(&self.margin_right, term_width);

//        debug!("curses:resize, TRBL: {}/{}/{}/{}", self.top, self.right, self.bottom, self.left);

        // width & height after margin calculated
        let screen_width = self.right - self.left;
        let screen_height = self.bottom - self.top;

        if screen_width < MIN_WIDTH || screen_height < MIN_HEIGHT {
            panic!("screen is two small with width: {}, height: {}", screen_width, screen_height);
        }

        let preview_width = Self::margin_to_fixed(&self.preview_size, screen_width);
        let preview_height = Self::margin_to_fixed(&self.preview_size, screen_height);

        if !self.preview_shown {
            self.win_main.reshape(self.top, self.right, self.bottom, self.left);
            self.win_preview.reshape(0, 0, 0, 0);
        } else {
            match self.preview_direction {
                Direction::Up => {
                    self.win_preview.reshape(self.top, self.right, self.top + preview_height, self.left);
                    self.win_main.reshape(self.top + preview_height, self.right, self.bottom, self.left);
                    self.win_preview.set_border(Some(Direction::Down));
                }
                Direction::Down => {
                    self.win_preview.reshape(self.bottom - preview_height, self.right, self.bottom, self.left);
                    self.win_main.reshape(self.top, self.right, self.bottom - preview_height, self.left);
                    self.win_preview.set_border(Some(Direction::Up));
                }
                Direction::Left => {
                    self.win_preview.reshape(self.top, self.left + preview_width, self.bottom, self.left);
                    self.win_main.reshape(self.top, self.right, self.bottom, self.left + preview_width);
                    self.win_preview.set_border(Some(Direction::Right));
                }
                Direction::Right => {
                    self.win_preview.reshape(self.top, self.right, self.bottom, self.right - preview_width);
                    self.win_main.reshape(self.top, self.right - preview_width, self.bottom, self.left);
                    self.win_preview.set_border(Some(Direction::Left));
                }
            }
        }
    }

    pub fn toggle_preview_window(&mut self) {
        self.preview_shown = !self.preview_shown;
    }

    pub fn close(&mut self) {
        self.win_preview.close();
        self.win_main.close();
        self.refresh();
    }

    pub fn refresh(&mut self) {
        self.win_preview.write_to_term(&self.term);
        self.win_main.write_to_term(&self.term);
        let _ = self.term.present();
    }
}

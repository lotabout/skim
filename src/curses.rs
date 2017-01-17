// An abstract layer towards ncurses-rs, which provides keycode, color scheme support
// Modeled after fzf

use ncurses::*;
use std::sync::RwLock;
use std::collections::HashMap;
use libc::{STDIN_FILENO, STDERR_FILENO, fdopen};

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

pub struct Curses {
    screen: SCREEN,
}

unsafe impl Send for Curses {}

impl Curses {
    pub fn new() -> Self {
        let local_conf = LcCategory::all;
        setlocale(local_conf, "en_US.UTF-8"); // for showing wide characters
        let stdin = unsafe { fdopen(STDIN_FILENO, "r".as_ptr() as *const i8)};
        let stderr = unsafe { fdopen(STDERR_FILENO, "w".as_ptr() as *const i8)};
        let screen = newterm(None, stderr, stdin);
        set_term(screen);
        //let screen = initscr();
        raw();
        noecho();

        Curses {
            screen: screen,
        }
    }

    fn get_color(&self, pair: i16, is_bold: bool) -> attr_t {
        if *USE_COLOR.read().unwrap() {
            attr_color(pair, is_bold)
        } else {
            attr_mono(pair, is_bold)
        }
    }

    pub fn mv(&self, y: i32, x: i32) {
        mv(y, x);
    }

    pub fn get_maxyx(&self) -> (i32, i32) {
        let mut max_y = 0;
        let mut max_x = 0;
        getmaxyx(stdscr(), &mut max_y, &mut max_x);
        (max_y, max_x)
    }

    pub fn getyx(&self) -> (i32, i32) {
        let mut y = 0;
        let mut x = 0;
        getyx(stdscr(), &mut y, &mut x);
        (y, x)
    }

    pub fn clrtoeol(&self) {
        clrtoeol();
    }

    pub fn endwin(&self) {
        endwin();
    }

    pub fn erase(&self) {
        erase();
    }

    pub fn cprint(&self, text: &str, pair: i16, is_bold: bool) {
        let attr = self.get_color(pair, is_bold);
        attron(attr);
        addstr(text);
        attroff(attr);
    }

    pub fn caddch(&self, ch: char, pair: i16, is_bold: bool) {
        let attr = self.get_color(pair, is_bold);
        attron(attr);
        addstr(&ch.to_string()); // to support wide character
        attroff(attr);
    }

    pub fn printw(&self, text: &str) {
        printw(text);
    }

    pub fn close(&self) {
        endwin();
        delscreen(self.screen);
    }

    pub fn attr_on(&self, attr: attr_t) {
        attron(attr);
    }

    pub fn attr_off(&self, attr: attr_t) {
        attroff(attr);
    }

    pub fn refresh(&self) {
        refresh();
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

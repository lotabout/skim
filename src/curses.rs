// An abstract layer towards ncurses-rs, which provides keycode, color scheme support
// Modeled after fzf

use ncurses::*;
use std::sync::RwLock;
use std::collections::HashMap;
use libc::{STDIN_FILENO, STDERR_FILENO, fdopen};

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
    static ref CURRENT_FG: RwLock<i16> = RwLock::new(7);
    static ref FG: RwLock<i16> = RwLock::new(7);
    static ref CURRENT_BG: RwLock<i16> = RwLock::new(0);
    static ref BG: RwLock<i16> = RwLock::new(0);
    static ref USE_COLOR: RwLock<bool> = RwLock::new(true);
}

pub fn init(theme: Option<&ColorTheme>, is_black: bool, _use_mouse: bool) {
    // initialize ncurses
    let local_conf = LcCategory::all;
    setlocale(local_conf, "en_US.UTF-8"); // for showing wide characters
    let mut use_color = USE_COLOR.write().unwrap();

    if let Some(theme) = theme {
        let base_theme = if tigetnum("colors") >= 256 {
            DARK256
        } else {
            DEFAULT16
        };

        init_pairs(&base_theme, &theme, is_black);
        *use_color = true;
    } else {
        *use_color = false;
    }
}

fn init_pairs(base: &ColorTheme, theme: &ColorTheme, is_black: bool) {
    let mut current_fg = CURRENT_FG.write().unwrap();
    let mut current_bg = CURRENT_BG.write().unwrap();
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
        assume_default_colors(shadow(base.fg, theme.fg) as i32, *bg as i32);
    }

    start_color();

    *current_fg = shadow(base.current, theme.current);
    *current_bg = shadow(base.dark_bg, theme.dark_bg);;
    init_pair(COLOR_PROMPT, shadow(base.prompt, theme.prompt), *bg);
    init_pair(COLOR_MATCHED, shadow(base.matched, theme.matched), *bg);
    init_pair(COLOR_CURRENT, shadow(base.current, theme.current), *current_bg);
    init_pair(COLOR_CURRENT_MATCH, shadow(base.current_match, theme.current_match), *current_bg);
    init_pair(COLOR_SPINNER, shadow(base.spinner, theme.spinner), *bg);
    init_pair(COLOR_INFO, shadow(base.info, theme.info), *bg);
    init_pair(COLOR_CURSOR, shadow(base.cursor, theme.cursor), *current_bg);
    init_pair(COLOR_SELECTED, shadow(base.selected, theme.selected), *current_bg);
    init_pair(COLOR_HEADER, shadow(base.header, theme.header), *bg);
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

impl Curses {
    pub fn new() -> Self {
        let stdin = unsafe { fdopen(STDIN_FILENO, "r".as_ptr() as *const i8)};
        let stderr = unsafe { fdopen(STDERR_FILENO, "w".as_ptr() as *const i8)};
        let screen = newterm(None, stderr, stdin);
        set_term(screen);
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

    pub fn get_yx(&self) -> (i32, i32) {
        let mut y = 0;
        let mut x = 0;
        getyx(stdscr, &mut y, &mut x);
        (y, x)
    }

    pub fn get_maxyx(&self) -> (i32, i32) {
        let mut max_y = 0;
        let mut max_x = 0;
        getmaxyx(stdscr, &mut max_y, &mut max_x);
        (max_y, max_x)
    }

    pub fn clrtoeol(&self) {
        clrtoeol();
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

}

// use default if x is COLOR_UNDEFINED, else use x
fn shadow(default: i16, x: i16) -> i16 {
    if x == COLOR_UNDEFINED { default } else { x }
}


fn attr_color(pair: i16, is_bold: bool) -> attr_t {
    let mut attr = 0;
    if pair > COLOR_NORMAL {
        attr = COLOR_PAIR(pair);
    }

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

pub struct ColorTheme {
    use_default: bool,
    fg: i16,
    bg: i16,
    dark_bg: i16,
    prompt: i16,
    matched: i16,
    current: i16,
    current_match: i16,
    spinner: i16,
    info: i16,
    cursor: i16,
    selected: i16,
    header: i16,
}

impl ColorTheme {
    pub fn new() -> Self {
        ColorTheme {
            use_default:  true,
            fg:            COLOR_UNDEFINED,
            bg:            COLOR_UNDEFINED,
            dark_bg:       COLOR_UNDEFINED,
            prompt:        COLOR_UNDEFINED,
            matched:       COLOR_UNDEFINED,
            current:       COLOR_UNDEFINED,
            current_match: COLOR_UNDEFINED,
            spinner:       COLOR_UNDEFINED,
            info:          COLOR_UNDEFINED,
            cursor:        COLOR_UNDEFINED,
            selected:      COLOR_UNDEFINED,
            header:        COLOR_UNDEFINED,

        }
    }
}

const DEFAULT16: ColorTheme = ColorTheme {
    use_default:   true,
    fg:            15,
    bg:            0,
    dark_bg:       COLOR_BLACK,
    prompt:        COLOR_BLUE,
    matched:       COLOR_GREEN,
    current:       COLOR_YELLOW,
    current_match: COLOR_GREEN,
    spinner:       COLOR_GREEN,
    info:          COLOR_WHITE,
    cursor:        COLOR_RED,
    selected:      COLOR_MAGENTA,
    header:        COLOR_CYAN,
};

const DARK256: ColorTheme = ColorTheme {
    use_default:   true,
    fg:            15,
    bg:            0,
    dark_bg:       236,
    prompt:        110,
    matched:       108,
    current:       254,
    current_match: 151,
    spinner:       148,
    info:          144,
    cursor:        161,
    selected:      168,
    header:        109,
};

// Not used for now, will later.
//const LIGHT256: ColorTheme = ColorTheme {
    //use_default:   true,
    //fg:            15,
    //bg:            0,
    //dark_bg:       251,
    //prompt:        25,
    //matched:       66,
    //current:       237,
    //current_match: 23,
    //spinner:       65,
    //info:          101,
    //cursor:        161,
    //selected:      168,
    //header:        31,
//};

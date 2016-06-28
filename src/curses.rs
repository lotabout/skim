// An abstract layer towards ncurses-rs, which provides keycode, color scheme support
// Modeled after fzf

use ncurses::*;

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
pub static COLOR_USER:          i16 = 10;

pub struct Curses {
    current_fg: i16,
    dark_bg: i16,
    use_color: bool,
}

impl Curses {
    pub fn new() -> Self {
        Curses {
            current_fg: COLOR_DEFAULT,
            dark_bg: COLOR_DEFAULT,
            use_color: false,
        }
    }
    pub fn init(&mut self, theme: Option<&ColorTheme>, is_black: bool, use_mouse: bool) {
        // initialize ncurses
        let local_conf = LcCategory::all;
        setlocale(local_conf, "en_US.UTF-8"); // for showing wide characters
        initscr();
        raw();
        keypad(stdscr, true);
        noecho();

        if let Some(theme) = theme {
            let base_theme = if tigetnum("colors") >= 256 {
                DARK256
            } else {
                DEFAULT16
            };

            self.init_pairs(&base_theme, &theme, is_black);
            self.use_color = true;
        } else {
            self.use_color = false;
        }
    }

    fn init_pairs(&mut self, base: &ColorTheme, theme: &ColorTheme, is_black: bool) {
        let mut fg = shadow(base.fg, theme.fg);
        let mut bg = shadow(base.bg, theme.bg);

        if is_black {
            bg = COLOR_BLACK;
        } else if theme.use_default {
            fg = COLOR_DEFAULT;
            bg = COLOR_DEFAULT;
            use_default_colors();
        }

        start_color();
        self.current_fg = shadow(base.current, theme.current);
        self.dark_bg = shadow(base.dark_bg, theme.dark_bg);;
        init_pair(COLOR_PROMPT, shadow(base.prompt, theme.prompt), bg);
        init_pair(COLOR_MATCHED, shadow(base.matched, theme.matched), bg);
        init_pair(COLOR_CURRENT, shadow(base.current, theme.current), self.dark_bg);
        init_pair(COLOR_CURRENT_MATCH, shadow(base.current_match, theme.current_match), self.dark_bg);
        init_pair(COLOR_SPINNER, shadow(base.spinner, theme.spinner), bg);
        init_pair(COLOR_INFO, shadow(base.info, theme.info), bg);
        init_pair(COLOR_CURSOR, shadow(base.cursor, theme.cursor), self.dark_bg);
        init_pair(COLOR_SELECTED, shadow(base.selected, theme.selected), self.dark_bg);
        init_pair(COLOR_HEADER, shadow(base.header, theme.header), bg);
    }

    pub fn get_color(&self, pair: i16, is_bold: bool) -> attr_t{
        if self.use_color {
            attr_color(pair, is_bold)
        } else {
            attr_mono(pair, is_bold)
        }
    }

    pub fn get_maxyx(&self) -> (i32, i32) {
        let mut max_y = 0;
        let mut max_x = 0;
        getmaxyx(stdscr, &mut max_y, &mut max_x);
        (max_y, max_x)
    }

    pub fn cprint(&self, pair: i16, is_bold: bool, text: &str) {
        let attr = self.get_color(pair, is_bold);
        attron(attr);
        addstr(text);
        attroff(attr);
    }

    pub fn caddch(&self, pair: i16, is_bold: bool, ch: char) {
        let attr = self.get_color(pair, is_bold);
        attron(attr);
        addstr(&ch.to_string()); // to support wide character
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

const LIGHT256: ColorTheme = ColorTheme {
    use_default:   true,
    fg:            15,
    bg:            0,
    dark_bg:       251,
    prompt:        25,
    matched:       66,
    current:       237,
    current_match: 23,
    spinner:       65,
    info:          101,
    cursor:        161,
    selected:      168,
    header:        31,
};

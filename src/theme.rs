///! Handle the color theme
use crate::options::SkimOptions;
use tuikit::prelude::*;

#[rustfmt::skip]
lazy_static! {
    pub static ref DEFAULT_THEME:  ColorTheme = ColorTheme::dark256();
}

/// The color scheme of skim's UI
///
/// <pre>
/// +----------------+
/// | >selected line |  --> selected & normal(fg/bg) & matched
/// |> current line  |  --> cursor & current & current_match
/// |  normal line   |
/// |\ 8/10          |  --> spinner & info
/// |> query         |  --> prompt & query
/// +----------------+
/// </pre>
#[rustfmt::skip]
#[derive(Copy, Clone, Debug)]
pub struct ColorTheme {
    fg:                   Color,
    bg:                   Color,
    normal_effect:        Effect,
    matched:              Color,
    matched_bg:           Color,
    matched_effect:       Effect,
    current:              Color,
    current_bg:           Color,
    current_effect:       Effect,
    current_match:        Color,
    current_match_bg:     Color,
    current_match_effect: Effect,
    query_fg:             Color,
    query_bg:             Color,
    query_effect:         Effect,
    spinner:              Color,
    info:                 Color,
    prompt:               Color,
    cursor:               Color,
    selected:             Color,
    header:               Color,
    border:               Color,
}

#[rustfmt::skip]
#[allow(dead_code)]
impl ColorTheme {
    pub fn init_from_options(options: &SkimOptions) -> ColorTheme {
        // register
        if let Some(color) = options.color {
            ColorTheme::from_options(color)
        } else {
            ColorTheme::dark256()
        }
    }

    fn empty() -> Self {
        ColorTheme {
            fg:                   Color::Default,
            bg:                   Color::Default,
            normal_effect:        Effect::empty(),
            matched:              Color::Default,
            matched_bg:           Color::Default,
            matched_effect:       Effect::empty(),
            current:              Color::Default,
            current_bg:           Color::Default,
            current_effect:       Effect::empty(),
            current_match:        Color::Default,
            current_match_bg:     Color::Default,
            current_match_effect: Effect::empty(),
            query_fg:             Color::Default,
            query_bg:             Color::Default,
            query_effect:         Effect::empty(),
            spinner:              Color::Default,
            info:                 Color::Default,
            prompt:               Color::Default,
            cursor:               Color::Default,
            selected:             Color::Default,
            header:               Color::Default,
            border:               Color::Default,
        }
    }

    fn bw() -> Self {
        ColorTheme {
            matched_effect:       Effect::UNDERLINE,
            current_effect:       Effect::REVERSE,
            current_match_effect: Effect::UNDERLINE | Effect::REVERSE,
            ..ColorTheme::empty()
        }
    }

    fn default16() -> Self {
        ColorTheme {
            matched:          Color::GREEN,
            matched_bg:       Color::BLACK,
            current:          Color::YELLOW,
            current_bg:       Color::BLACK,
            current_match:    Color::GREEN,
            current_match_bg: Color::BLACK,
            spinner:          Color::GREEN,
            info:             Color::WHITE,
            prompt:           Color::BLUE,
            cursor:           Color::RED,
            selected:         Color::MAGENTA,
            header:           Color::CYAN,
            border:           Color::LIGHT_BLACK,
            ..ColorTheme::empty()
        }
    }

    fn dark256() -> Self {
        ColorTheme {
            matched:          Color::AnsiValue(108),
            matched_bg:       Color::AnsiValue(0),
            current:          Color::AnsiValue(254),
            current_bg:       Color::AnsiValue(236),
            current_match:    Color::AnsiValue(151),
            current_match_bg: Color::AnsiValue(236),
            spinner:          Color::AnsiValue(148),
            info:             Color::AnsiValue(144),
            prompt:           Color::AnsiValue(110),
            cursor:           Color::AnsiValue(161),
            selected:         Color::AnsiValue(168),
            header:           Color::AnsiValue(109),
            border:           Color::AnsiValue(59),
            ..ColorTheme::empty()
        }
    }

    fn molokai256() -> Self {
        ColorTheme {
            matched:          Color::AnsiValue(234),
            matched_bg:       Color::AnsiValue(186),
            current:          Color::AnsiValue(254),
            current_bg:       Color::AnsiValue(236),
            current_match:    Color::AnsiValue(234),
            current_match_bg: Color::AnsiValue(186),
            spinner:          Color::AnsiValue(148),
            info:             Color::AnsiValue(144),
            prompt:           Color::AnsiValue(110),
            cursor:           Color::AnsiValue(161),
            selected:         Color::AnsiValue(168),
            header:           Color::AnsiValue(109),
            border:           Color::AnsiValue(59),
            ..ColorTheme::empty()
        }
    }

    fn light256() -> Self {
        ColorTheme {
            matched:          Color::AnsiValue(0),
            matched_bg:       Color::AnsiValue(220),
            current:          Color::AnsiValue(237),
            current_bg:       Color::AnsiValue(251),
            current_match:    Color::AnsiValue(66),
            current_match_bg: Color::AnsiValue(251),
            spinner:          Color::AnsiValue(65),
            info:             Color::AnsiValue(101),
            prompt:           Color::AnsiValue(25),
            cursor:           Color::AnsiValue(161),
            selected:         Color::AnsiValue(168),
            header:           Color::AnsiValue(31),
            border:           Color::AnsiValue(145),
            ..ColorTheme::empty()
        }
    }

    fn from_options(color: &str) -> Self {
        let mut theme = ColorTheme::dark256();
        for pair in color.split(',') {
            let color: Vec<&str> = pair.split(':').collect();
            if color.len() < 2 {
                theme = match color[0] {
                    "molokai"  => ColorTheme::molokai256(),
                    "light"    => ColorTheme::light256(),
                    "16"       => ColorTheme::default16(),
                    "bw"       => ColorTheme::bw(),
                    "empty"    => ColorTheme::empty(),
                    "dark" | "default" | _ => ColorTheme::dark256(),
                };
                continue;
            }

            let new_color = if color[1].len() == 7 {
                // 256 color
                let r = u8::from_str_radix(&color[1][1..3], 16).unwrap_or(255);
                let g = u8::from_str_radix(&color[1][3..5], 16).unwrap_or(255);
                let b = u8::from_str_radix(&color[1][5..7], 16).unwrap_or(255);
                Color::Rgb(r, g, b)
            } else {
                color[1].parse::<u8>()
                    .map(Color::AnsiValue)
                    .unwrap_or(Color::Default)
            };

            match color[0] {
                "fg"                    => theme.fg               = new_color,
                "bg"                    => theme.bg               = new_color,
                "matched" | "hl"        => theme.matched          = new_color,
                "matched_bg"            => theme.matched_bg       = new_color,
                "current" | "fg+"       => theme.current          = new_color,
                "current_bg" | "bg+"    => theme.current_bg       = new_color,
                "current_match" | "hl+" => theme.current_match    = new_color,
                "current_match_bg"      => theme.current_match_bg = new_color,
                "query"                 => theme.query_fg         = new_color,
                "query_bg"              => theme.query_bg         = new_color,
                "spinner"               => theme.spinner          = new_color,
                "info"                  => theme.info             = new_color,
                "prompt"                => theme.prompt           = new_color,
                "cursor" | "pointer"    => theme.cursor           = new_color,
                "selected" | "marker"   => theme.selected         = new_color,
                "header"                => theme.header           = new_color,
                "border"                => theme.border           = new_color,
                _ => {}
            }
        }
        theme
    }

    pub fn normal(&self) -> Attr {
        Attr {
            fg: self.fg,
            bg: self.bg,
            effect: self.normal_effect,
        }
    }

    pub fn matched(&self) -> Attr {
        Attr {
            fg: self.matched,
            bg: self.matched_bg,
            effect: self.matched_effect,
        }
    }

    pub fn current(&self) -> Attr {
        Attr {
            fg: self.current,
            bg: self.current_bg,
            effect: self.current_effect,
        }
    }

    pub fn current_match(&self) -> Attr {
        Attr {
            fg: self.current_match,
            bg: self.current_match_bg,
            effect: self.current_match_effect,
        }
    }

    pub fn query(&self) -> Attr {
        Attr {
            fg: self.query_fg,
            bg: self.query_bg,
            effect: self.query_effect,
        }
    }

    pub fn spinner(&self) -> Attr {
        Attr {
            fg: self.spinner,
            bg: self.bg,
            effect: Effect::BOLD,
        }
    }

    pub fn info(&self) -> Attr {
        Attr {
            fg: self.info,
            bg: self.bg,
            effect: Effect::empty(),
        }
    }

    pub fn prompt(&self) -> Attr {
        Attr {
            fg: self.prompt,
            bg: self.bg,
            effect: Effect::empty(),
        }
    }

    pub fn cursor(&self) -> Attr {
        Attr {
            fg: self.cursor,
            bg: self.current_bg,
            effect: Effect::empty(),
        }
    }

    pub fn selected(&self) -> Attr {
        Attr {
            fg: self.selected,
            bg: self.current_bg,
            effect: Effect::empty(),
        }
    }

    pub fn header(&self) -> Attr {
        Attr {
            fg: self.header,
            bg: self.bg,
            effect: Effect::empty(),
        }
    }

    pub fn border(&self) -> Attr {
        Attr {
            fg: self.border,
            bg: self.bg,
            effect: Effect::empty(),
        }
    }
}

use crate::field::get_string_by_range;
use lazy_string_replace::LazyReplace;
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::cmp::min;
use std::fmt::Display;
use std::prelude::v1::*;
use tuikit::prelude::*;
use unicode_width::UnicodeWidthChar;

lazy_static! {
    static ref RE_FIELDS: Regex = Regex::new(r"\\?(\{ *-?[0-9.,cq+]*? *})").unwrap();
}

pub fn escape_single_quote(text: &str) -> impl Display + '_ {
    text.lazy_replace('\'', "'\\''")
}

/// use to print a single line, properly handle the tabsteop and shift of a string
/// e.g. a long line will be printed as `..some content` or `some content..` or `..some content..`
/// depends on the container's width and the size of the content.
///
/// ```text
/// let's say we have a very long line with lots of useless information
///                                |.. with lots of use..|             // only to show this
///                                |<- container width ->|
///             |<-    shift    -> |
/// |< hscroll >|
/// ```

pub struct LinePrinter {
    start: usize,
    end: usize,
    current_pos: i32,
    screen_col: usize,

    // start position
    row: usize,
    col: usize,

    tabstop: usize,
    shift: usize,
    text_width: usize,
    container_width: usize,
    hscroll_offset: usize,
}

impl LinePrinter {
    pub fn builder() -> Self {
        LinePrinter {
            start: 0,
            end: 0,
            current_pos: -1,
            screen_col: 0,

            row: 0,
            col: 0,

            tabstop: 8,
            shift: 0,
            text_width: 0,
            container_width: 0,
            hscroll_offset: 0,
        }
    }

    pub fn row(mut self, row: usize) -> Self {
        self.row = row;
        self
    }

    pub fn col(mut self, col: usize) -> Self {
        self.col = col;
        self
    }

    pub fn tabstop(mut self, tabstop: usize) -> Self {
        self.tabstop = tabstop;
        self
    }

    pub fn hscroll_offset(mut self, offset: usize) -> Self {
        self.hscroll_offset = offset;
        self
    }

    pub fn text_width(mut self, width: usize) -> Self {
        self.text_width = width;
        self
    }

    pub fn container_width(mut self, width: usize) -> Self {
        self.container_width = width;
        self
    }

    pub fn shift(mut self, shift: usize) -> Self {
        self.shift = shift;
        self
    }

    pub fn build(mut self) -> Self {
        self.reset();
        self
    }

    pub fn reset(&mut self) {
        self.current_pos = 0;
        self.screen_col = self.col;

        self.start = self.shift + self.hscroll_offset;
        self.end = self.start + self.container_width;
    }

    fn print_ch_to_canvas(&mut self, canvas: &mut dyn Canvas, ch: char, attr: Attr, skip: bool) {
        let w = ch.width().unwrap_or(2);

        if !skip {
            let _ = canvas.put_cell(self.row, self.screen_col, Cell::default().ch(ch).attribute(attr));
        }

        self.screen_col += w;
    }

    fn print_char_raw(&mut self, canvas: &mut dyn Canvas, ch: char, attr: Attr, skip: bool) {
        // hide the content that outside the screen, and show the hint(i.e. `..`) for overflow
        // the hidden character

        let w = ch.width().unwrap_or(2);

        assert!(self.current_pos >= 0);
        let current = self.current_pos as usize;

        if current < self.start || current >= self.end {
            // pass if it is hidden
        } else if current < self.start + 2 && (self.shift > 0 || self.hscroll_offset > 0) {
            // print left ".."
            for _ in 0..min(w, current - self.start + 1) {
                self.print_ch_to_canvas(canvas, '.', attr, skip);
            }
        } else if self.end - current <= 2 && (self.text_width > self.end) {
            // print right ".."
            for _ in 0..min(w, self.end - current) {
                self.print_ch_to_canvas(canvas, '.', attr, skip);
            }
        } else {
            self.print_ch_to_canvas(canvas, ch, attr, skip);
        }

        self.current_pos += w as i32;
    }

    pub fn print_char(&mut self, canvas: &mut dyn Canvas, ch: char, attr: Attr, skip: bool) {
        if ch != '\t' {
            self.print_char_raw(canvas, ch, attr, skip);
        } else {
            // handle tabstop
            let rest = if self.current_pos < 0 {
                self.tabstop
            } else {
                self.tabstop - (self.current_pos as usize) % self.tabstop
            };
            for _ in 0..rest {
                self.print_char_raw(canvas, ' ', attr, skip);
            }
        }
    }
}

/// return an array, arr[i] store the display width till char[i]
pub fn accumulate_text_width(text: &str, tabstop: usize) -> Vec<usize> {
    let mut ret = Vec::new();
    let mut w = 0;
    for ch in text.chars() {
        w += if ch == '\t' {
            tabstop - (w % tabstop)
        } else {
            ch.width().unwrap_or(2)
        };
        ret.push(w);
    }
    ret
}

/// "smartly" calculate the "start" position of the string in order to show the matched contents
/// for example, if the match appear in the end of a long string, we need to show the right part.
/// ```text
/// xxxxxxxxxxxxxxxxxxxxxxxxxxMMxxxxxMxxxxx
///               shift ->|               |
/// ```
///
/// return (left_shift, full_print_width)
pub fn reshape_string(
    text: &str,
    container_width: usize,
    match_start: usize,
    match_end: usize,
    tabstop: usize,
) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }

    let acc_width = accumulate_text_width(text, tabstop);
    let full_width = acc_width[acc_width.len() - 1];
    if full_width <= container_width {
        return (0, full_width);
    }

    // w1, w2, w3 = len_before_matched, len_matched, len_after_matched
    let w1 = if match_start == 0 {
        0
    } else {
        acc_width[match_start - 1]
    };
    let w2 = if match_end >= acc_width.len() {
        full_width - w1
    } else {
        acc_width[match_end] - w1
    };
    let w3 = acc_width[acc_width.len() - 1] - w1 - w2;

    if (w1 > w3 && w2 + w3 <= container_width) || (w3 <= 2) {
        // right-fixed
        //(right_fixed(&acc_width, container_width), full_width)
        (full_width - container_width, full_width)
    } else if w1 <= w3 && w1 + w2 <= container_width {
        // left-fixed
        (0, full_width)
    } else {
        // left-right
        (acc_width[match_end] - container_width + 2, full_width)
    }
}

/// margin option string -> Size
/// 10 -> Size::Fixed(10)
/// 10% -> Size::Percent(10)
pub fn margin_string_to_size(margin: &str) -> Size {
    if margin.ends_with('%') {
        Size::Percent(min(100, margin[0..margin.len() - 1].parse::<usize>().unwrap_or(100)))
    } else {
        Size::Fixed(margin.parse::<usize>().unwrap_or(0))
    }
}

/// Parse margin configuration, e.g.
/// - `TRBL`     Same  margin  for  top,  right, bottom, and left
/// - `TB,RL`    Vertical, horizontal margin
/// - `T,RL,B`   Top, horizontal, bottom margin
/// - `T,R,B,L`  Top, right, bottom, left margin
pub fn parse_margin(margin_option: &str) -> (Size, Size, Size, Size) {
    let margins = margin_option.split(',').collect::<Vec<&str>>();

    match margins.len() {
        1 => {
            let margin = margin_string_to_size(margins[0]);
            (margin, margin, margin, margin)
        }
        2 => {
            let margin_tb = margin_string_to_size(margins[0]);
            let margin_rl = margin_string_to_size(margins[1]);
            (margin_tb, margin_rl, margin_tb, margin_rl)
        }
        3 => {
            let margin_top = margin_string_to_size(margins[0]);
            let margin_rl = margin_string_to_size(margins[1]);
            let margin_bottom = margin_string_to_size(margins[2]);
            (margin_top, margin_rl, margin_bottom, margin_rl)
        }
        4 => {
            let margin_top = margin_string_to_size(margins[0]);
            let margin_right = margin_string_to_size(margins[1]);
            let margin_bottom = margin_string_to_size(margins[2]);
            let margin_left = margin_string_to_size(margins[3]);
            (margin_top, margin_right, margin_bottom, margin_left)
        }
        _ => (Size::Fixed(0), Size::Fixed(0), Size::Fixed(0), Size::Fixed(0)),
    }
}

/// The context for injecting command.
#[derive(Copy, Clone)]
pub struct InjectContext<'a> {
    pub delimiter: &'a Regex,
    pub current_selection: &'a str,
    pub selections: &'a [&'a str],
    pub query: &'a str,
    pub cmd_query: &'a str,
}

/// inject the fields into commands
/// cmd: `echo {1..}`, text: `a,b,c`, delimiter: `,`
/// => `echo b,c`
///
/// * `{}` for current selection
/// * `{1..}`, etc. for fields
/// * `{+}` for all selections
/// * `{q}` for query
/// * `{cq}` for command query
pub fn inject_command<'a>(cmd: &'a str, context: InjectContext<'a>) -> Cow<'a, str> {
    RE_FIELDS.replace_all(cmd, |caps: &Captures| {
        // \{...
        if &caps[0][0..1] == "\\" {
            return caps[0].to_string();
        }

        // {1..} and other variant
        let range = &caps[1];
        assert!(range.len() >= 2);
        let range = &range[1..range.len() - 1];
        let range = range.trim();

        if range == "+" {
            let current_selection = vec![context.current_selection];
            let selections = if context.selections.is_empty() {
                &current_selection
            } else {
                context.selections
            };

            return selections
                .iter()
                .map(|&s| format!("'{}'", escape_single_quote(s)))
                .collect::<Vec<_>>()
                .join(" ");
        }

        let replacement = match range {
            "" => context.current_selection,
            "+" => unreachable!(),
            "q" => context.query,
            "cq" => context.cmd_query,
            _ => get_string_by_range(context.delimiter, context.current_selection, range).unwrap_or(""),
        };

        format!("'{}'", escape_single_quote(replacement))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accumulate_text_width() {
        assert_eq!(accumulate_text_width("abcdefg", 8), vec![1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(accumulate_text_width("ab中de国g", 8), vec![1, 2, 4, 5, 6, 8, 9]);
        assert_eq!(accumulate_text_width("ab\tdefg", 8), vec![1, 2, 8, 9, 10, 11, 12]);
        assert_eq!(accumulate_text_width("ab中\te国g", 8), vec![1, 2, 4, 8, 9, 11, 12]);
    }

    #[test]
    fn test_reshape_string() {
        // no match, left fixed to 0
        assert_eq!(reshape_string("abc", 10, 0, 0, 8), (0, 3));
        assert_eq!(reshape_string("a\tbc", 8, 0, 0, 8), (0, 10));
        assert_eq!(reshape_string("a\tb\tc", 10, 0, 0, 8), (0, 17));
        assert_eq!(reshape_string("a\t中b\tc", 8, 0, 0, 8), (0, 17));
        assert_eq!(reshape_string("a\t中b\tc012345", 8, 0, 0, 8), (0, 23));
    }

    #[test]
    fn test_inject_command() {
        let delimiter = Regex::new(r",").unwrap();
        let current_selection = "a,b,c";
        let selections = vec!["a,b,c", "x,y,z"];
        let query = "query";
        let cmd_query = "cmd_query";

        let default_context = InjectContext {
            delimiter: &delimiter,
            current_selection,
            selections: &selections,
            query,
            cmd_query,
        };

        assert_eq!("'a,b,c'", inject_command("{}", default_context));
        assert_eq!("'a,b,c'", inject_command("{ }", default_context));

        assert_eq!("'a'", inject_command("{1}", default_context));
        assert_eq!("'b'", inject_command("{2}", default_context));
        assert_eq!("'c'", inject_command("{3}", default_context));
        assert_eq!("''", inject_command("{4}", default_context));
        assert_eq!("'c'", inject_command("{-1}", default_context));
        assert_eq!("'b'", inject_command("{-2}", default_context));
        assert_eq!("'a'", inject_command("{-3}", default_context));
        assert_eq!("''", inject_command("{-4}", default_context));
        assert_eq!("'a,b'", inject_command("{1..2}", default_context));
        assert_eq!("'b,c'", inject_command("{2..}", default_context));

        assert_eq!("'query'", inject_command("{q}", default_context));
        assert_eq!("'cmd_query'", inject_command("{cq}", default_context));
        assert_eq!("'a,b,c' 'x,y,z'", inject_command("{+}", default_context));
    }
}

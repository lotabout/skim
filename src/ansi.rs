// Parse ANSI attr code

use curses::{attr_t, register_ansi, Window};
use regex::Regex;
use std::default::Default;

pub struct ANSIParser {
    re: &'static Regex,
    last_attr: Option<attr_t>,
}

lazy_static! {
    static ref ANSI_RE: Regex =
        Regex::new(r"\x1B\[(?:([0-9]+;[0-9]+[Hf])|([0-9]+[ABCD])|(s|u|2J|K)|([0-9;]*m)|(=[0-9]+[hI]))").unwrap();
}

impl Default for ANSIParser {
    fn default() -> Self {
        ANSIParser {
            re: &ANSI_RE,
            last_attr: None,
        }
    }
}

#[derive(Clone, Debug)]
// named like to not clash with ANSIString from ansi_term crate
pub struct AnsiString {
    pub stripped: String,
    pub ansi_states: Vec<(usize, attr_t)>
}

impl AnsiString{
    pub fn new_empty() -> AnsiString {
        AnsiString {
            stripped: "".to_string(),
            ansi_states: Vec::new()
        }
    }

    pub fn into_inner(self) -> String{
        self.stripped
    }

    pub fn print(&self, curses: &mut Window) {

        for (c, attrs) in self.iter(){
            for a in attrs{
                curses.attr_on(a)
            }
            curses.addch(c);
        };
    }

    pub fn iter<'a>(&'a self) -> AnsiStringIterator<'a> {
        AnsiStringIterator {
            it_text: Box::new(self.stripped.chars().enumerate()),
            pk_ansi_states: self.ansi_states.iter().peekable()
        }
    }
}

use std::iter::Enumerate;
use std::iter::Peekable;
pub struct AnsiStringIterator<'a> {
    it_text: Box<Enumerate<std::str::Chars<'a>>>,
    pk_ansi_states: std::iter::Peekable<std::slice::Iter<'a, (usize, u16)>>
}

impl<'a> Iterator for AnsiStringIterator<'a>{
    type Item = (char, Vec<attr_t>);
    fn next(&mut self) -> Option<Self::Item>{
        let mut res: Vec<attr_t> = Vec::new();
        let (ch_idx, ch) = match self.it_text.next(){
            Some((ch_idx, ch)) => (ch_idx, ch),
            None => {return None;}
        };

        while let Some(&&(ansi_idx, attr)) = self.pk_ansi_states.peek() {
            if ch_idx == ansi_idx {
                res.push(attr);
                let _ = self.pk_ansi_states.next();
            } else if ch_idx > ansi_idx {
                let _ = self.pk_ansi_states.next();
            } else {
                break;
            }
        }
        return Some((ch, res));
    }
}


impl ANSIParser {
    pub fn parse_ansi(&mut self, text: &str) -> AnsiString {
        let mut strip_string = String::new();
        let mut colors = Vec::new();

        // assume parse_ansi is called linewise.
        // Because ANSI color code can affect text of next lines. We will save the last attribute and
        // add it to the newest line if no new color is specified.
        match self.re.find(text) {
            Some(mat) if mat.start() == 0 => {}
            Some(_) | None => {
                self.last_attr.map(|attr| {
                    colors.push((0, attr));
                });
            }
        }

        let mut num_chars = 0;
        let mut last = 0;
        for mat in self.re.find_iter(text) {
            let (start, end) = (mat.start(), mat.end());
            strip_string.push_str(&text[last..start]);
            num_chars += (&text[last..start]).chars().count();

            last = end;

            let attr = self.interpret_code(&text[start..end]);
            attr.map(|attr| {
                colors.push((num_chars, attr));
            });
            self.last_attr = attr;
        }

        strip_string.push_str(&text[last..text.len()]);

        AnsiString{
            stripped: strip_string,
            ansi_states: colors
        }
    }

    fn interpret_code(&self, code: &str) -> Option<attr_t> {
        if code == "\x1B[K" || code == "\x1B[2J" {
            // clear screen & clear line
            None
        } else {
            let key = register_ansi(code.to_owned());
            Some(key)
        }

        //let mut state256 = 0;
        //let mut attr = 0;
        //let mut fg = -1;
        //let mut bg = -1;
        //let mut use_fg = true;

        //let code = &code[2..code.len()-1]; // ^[[1;30;40m -> 1;30;40
        //if code.is_empty() {
        //return Some(A_NORMAL());
        //}

        //for num in code.split(';').map(|x| x.parse::<i16>()) {
        //match state256 {
        //0 => {
        //match num.unwrap_or(0) {
        //0 => {attr = 0;}
        //1 => {attr |= A_BOLD();}
        //4 => {attr |= A_UNDERLINE();}
        //5 => {attr |= A_BLINK();}
        //7 => {attr |= A_REVERSE();}
        //8 => {attr |= A_INVIS();}
        //38 => {
        //use_fg = true;
        //state256 += 1;
        //}
        //48 => {
        //use_fg = false;
        //state256 += 1;
        //}
        //39 => {
        //fg = -1;
        //}
        //49 => {
        //bg = -1;
        //}
        //num if num >= 30 && num <= 37 => {
        //fg = num - 30;
        //}
        //num if num >= 40 && num <= 47 => {
        //bg = num - 40;
        //}
        //_ => {
        //}
        //}
        //}
        //1 => {
        //match num.unwrap_or(0) {
        //5 => { state256 += 1; }
        //_ => { state256 = 0; }
        //}
        //}
        //2 => {
        //if use_fg {
        //fg = num.unwrap_or(-1);
        //} else {
        //bg = num.unwrap_or(-1);
        //}
        //}
        //_ => {}
        //}
        //}

        //if fg != -1 || bg != -1 {
        //attr |= get_color_pair(fg, bg);
        //}

        //Some(attr)
    }
}

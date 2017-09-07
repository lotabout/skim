// Parse ANSI attr code

use regex::Regex;
use curses::{register_ansi, attr_t};
use std::sync::Mutex;

pub fn parse_ansi(text: &str) -> (String, Vec<(usize, attr_t)>) {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"\x1b\[[0-9;]*[mK]").unwrap();
        static ref LAST_ATTR: Mutex<Option<attr_t>> = Mutex::new(None);
    }

    let mut strip_string = String::new();
    let mut colors = Vec::new();
    let mut last_attr = LAST_ATTR.lock().unwrap();

    // assume parse_ansi is called linewise.
    // Because ANSI color code can affect text of next lines. We will save the last attribute and
    // add it to the newest line if no new color is specified.
    match RE.find(text) {
        Some(mat) if mat.start() == 0 => {}
        Some(_) | None => {
            last_attr.map(|attr| {
                colors.push((0, attr));
            });
        }
    }

    let mut num_chars = 0;
    let mut last = 0;
    for mat in RE.find_iter(text) {
        let (start, end) = (mat.start(), mat.end());
        strip_string.push_str(&text[last..start]);
        num_chars += (&text[last..start]).chars().count();

        last = end;

        let attr = interpret_code(&text[start..end]);
        attr.map(|attr| {
            colors.push((num_chars, attr));
        });
        *last_attr = attr;
    }

    strip_string.push_str(&text[last..text.len()]);

    (strip_string, colors)
}

fn interpret_code(code: &str) -> Option<attr_t> {
    if &code[code.len()-1..code.len()] == "K"{
        return None;
    }

    let key = register_ansi(code.to_owned());
    Some(key)

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

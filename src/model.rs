use std::sync::mpsc::{Receiver, Sender};
use event::{Event, EventArg};
use item::{MatchedItem, MatchedRange};
use std::time::Instant;
use std::cmp::{max, min};
use orderedvec::OrderedVec;
use std::sync::Arc;
use std::collections::HashMap;

use curses::{ColorTheme, Curses};
use curses::*;
use curses;
use getopts;

use std::io::Write;
macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

pub type ClosureType = Box<Fn(&Curses) + Send>;

const SPINNER_DURATION: u32 = 200;
const SPINNERS: [char; 8] = ['-', '\\', '|', '/', '-', '\\', '|', '/'];

pub struct Model {
    rx_cmd: Receiver<(Event, EventArg)>,
    items: OrderedVec<Arc<MatchedItem>>, // all items
    total_item: usize,
    selected: HashMap<(usize, usize), Arc<MatchedItem>>,

    item_cursor: usize, // the index of matched item currently highlighted.
    line_cursor: usize, // line No.
    hscroll_offset: usize,
    reverse: bool,
    height: i32,
    width: i32,

    multi_selection: bool,
    pub tabstop: usize,

    reader_stopped: bool,
    sender_stopped: bool,
    timer: Instant,
    theme: ColorTheme,
}

impl Model {
    pub fn new(rx_cmd: Receiver<(Event, EventArg)>) -> Self {
        Model {
            rx_cmd: rx_cmd,
            items: OrderedVec::new(),
            total_item: 0,
            selected: HashMap::new(),

            item_cursor: 0,
            line_cursor: 0,
            hscroll_offset: 0,
            reverse: false,
            height: 0,
            width: 0,

            multi_selection: false,
            tabstop: 8,

            reader_stopped: false,
            sender_stopped: false,
            timer: Instant::now(),
            theme: ColorTheme::new(),
        }
    }

    pub fn parse_options(&mut self, options: &getopts::Matches) {
        if options.opt_present("m") {
            self.multi_selection = true;
        }

        if options.opt_present("no-multi") {
            self.multi_selection = false;
        }

        if let Some(color) = options.opt_str("color") {
            self.theme = ColorTheme::from_options(&color);
        }
    }

    pub fn run(&mut self) {
        // generate a new instance of curses for printing

        let curses = Curses::new();
        curses::init(Some(&self.theme), false, false);

        // main loop
        loop {
            // check for new item
            if let Ok((ev, arg)) = self.rx_cmd.recv() {
                match ev {
                    Event::EvModelNewItem => {
                        let item = *arg.downcast::<MatchedItem>().unwrap();
                        self.new_item(item);
                    }

                    Event::EvModelRestart => {
                        // clean the model
                        self.clean_model();
                        self.update_size(&curses);
                    }

                    Event::EvModelRedraw => {
                        self.update_size(&curses);

                        let print_query = *arg.downcast::<ClosureType>().unwrap();
                        curses.erase();
                        self.print_screen(&curses, print_query);
                        curses.refresh();
                    }

                    Event::EvModelNotifyTotal => {
                        if ! self.reader_stopped {
                            self.total_item = *arg.downcast::<usize>().unwrap();
                        }
                    }

                    Event::EvSenderStopped => {
                        self.sender_stopped = true;
                    }
                    Event::EvReaderStopped => { self.reader_stopped = true; }
                    Event::EvReaderStarted => { self.reader_stopped = false; }

                    //---------------------------------------------------------
                    // Actions

                    Event::EvActAccept => {
                        curses.close();

                        // output the expect key
                        let (accept_key, tx_ack): (Option<String>, Sender<usize>) = *arg.downcast().unwrap();
                        accept_key.map(|key| {
                            println!("{}", key);
                        });

                        self.act_output();

                        let _ = tx_ack.send(self.selected.len());
                        break;
                    }
                    Event::EvActUp => {
                        self.act_move_line_cursor(1);
                    }
                    Event::EvActDown => {
                        self.act_move_line_cursor(-1);
                    }
                    Event::EvActToggle => {
                        self.act_toggle();
                    }
                    Event::EvActToggleDown => {
                        self.act_toggle();
                        self.act_move_line_cursor(-1);
                    }
                    Event::EvActToggleUp => {
                        self.act_toggle();
                        self.act_move_line_cursor(1);
                    }
                    Event::EvActToggleAll => {
                        self.act_toggle_all();
                    }
                    Event::EvActSelectAll => {
                        self.act_select_all();
                    }
                    Event::EvActDeselectAll => {
                        self.act_deselect_all();
                    }
                    Event::EvActPageDown => {
                        let height = 1-self.height;
                        self.act_move_line_cursor(height);
                    }
                    Event::EvActPageUp => {
                        let height = self.height-1;
                        self.act_move_line_cursor(height);
                    }
                    Event::EvActScrollLeft => {
                        self.act_scroll(*arg.downcast::<i32>().unwrap_or(Box::new(-2)));
                    }
                    Event::EvActScrollRight => {
                        self.act_scroll(*arg.downcast::<i32>().unwrap_or(Box::new(2)));
                    }

                    _ => {}
                }
            }
        }
    }

    fn clean_model(&mut self) {
        self.items.clear();
        self.item_cursor = 0;
        self.line_cursor = 0;
        self.hscroll_offset = 0;
        self.sender_stopped = false;
        if !self.reader_stopped {
            self.total_item = 0;
        }
    }

    fn update_size(&mut self, curses: &Curses) {
        // update the (height, width)
        let (h, w) = curses.get_maxyx();
        self.height = h-2;
        self.width = w-2;
    }

    fn new_item(&mut self, item: MatchedItem) {
        self.items.push(Arc::new(item));
    }

    fn print_screen(&mut self, curses: &Curses, print_query: ClosureType) {
        let (h, w) = curses.get_maxyx();
        let (h, _) = (h as usize, w as usize);

        // screen-line: y         <--->   item-line: (height - y - 1)
        //              h-1                          h-(h-1)-1 = 0
        //              0                            h-1
        // screen-line: (h-l-1)   <--->   item-line: l

        let lower = self.item_cursor;
        let upper = min(self.item_cursor + h-2, self.items.len());

        for i in lower..upper {
            let l = i - lower;
            curses.mv((if self.reverse {l+2} else {h-3 - l} ) as i32, 0);
            // print the cursor label
            let label = if l == self.line_cursor {">"} else {" "};
            curses.cprint(label, COLOR_CURSOR, true);

            let item = self.items.get(i).unwrap().clone();
            self.print_item(curses, &item, l == self.line_cursor);
        }

        // print status line
        self.print_status_line(curses);

        // print query
        curses.mv((if self.reverse {0} else {h-1}) as i32, 0);
        print_query(curses);
    }

    fn print_status_line(&self, curses: &Curses) {
        curses.mv(if self.reverse {1} else {self.height} , 0);
        curses.clrtoeol();

        // display spinner
        if self.reader_stopped && self.sender_stopped {
            self.print_char(curses, ' ', COLOR_NORMAL, false);
        } else {
            let time = self.timer.elapsed();
            let mills = (time.as_secs()*1000) as u32 + time.subsec_nanos()/1000/1000;
            let index = (mills / SPINNER_DURATION) % (SPINNERS.len() as u32);
            self.print_char(curses, SPINNERS[index as usize], COLOR_SPINNER, true);
        }

        // display matched/total number
        curses.cprint(format!(" {}/{}", self.items.len(), self.total_item).as_ref(), COLOR_INFO, false);

        // selected number
        if self.multi_selection && !self.selected.is_empty() {
            curses.cprint(format!(" [{}]", self.selected.len()).as_ref(), COLOR_INFO, true);
        }

        // item cursor
        let line_num_str = format!(" {} ", self.item_cursor+self.line_cursor);
        curses.mv(if self.reverse {1} else {self.height}, self.width - (line_num_str.len() as i32));
        curses.cprint(&line_num_str, COLOR_INFO, true);
    }

    fn print_item(&self, curses: &Curses, matched_item: &MatchedItem, is_current: bool) {
        let index = matched_item.item.get_full_index();

        if self.selected.contains_key(&index) {
            curses.cprint(">", COLOR_SELECTED, true);
        } else {
            curses.cprint(" ", if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, false);
        }

        match matched_item.matched_range {
            Some(MatchedRange::Chars(ref matched_indics)) => {
                let (matched_start_pos, matched_end_pos) = if !matched_indics.is_empty() {
                    (matched_indics[0], matched_indics[matched_indics.len()-1] + 1)
                } else {
                    (0, 1)
                };

                let item = &matched_item.item;
                let (text, mut idx) = reshape_string(&item.get_text().chars().collect::<Vec<char>>(),
                                                     (self.width-3) as usize,
                                                     self.hscroll_offset,
                                                     matched_start_pos,
                                                     matched_end_pos);
                let mut matched_indics_iter = matched_indics.iter().peekable();
                let mut ansi_states = item.get_ansi_states().iter().peekable();

                // skip indics
                while let Some(&&index) = matched_indics_iter.peek() {
                    if idx > index {
                        let _ = matched_indics_iter.next();
                    } else {
                        break;
                    }
                }

                // skip ansi states
                let mut last_attr = 0;
                while let Some(&&(index, attr)) = ansi_states.peek() {
                    if idx > index {
                        last_attr = attr;
                        let _ = ansi_states.next();
                    } else {
                        break;
                    }
                }
                curses.attr_on(last_attr);

                for &ch in &text {
                    match matched_indics_iter.peek() {
                        Some(&&index) if idx == index => {
                            self.print_char(curses, ch, COLOR_MATCHED, is_current);
                            let _ = matched_indics_iter.next();
                        }
                        Some(_) | None => {
                            match ansi_states.peek() {
                                Some(&&(index, attr)) if idx == index => {
                                    // print ansi color codes.
                                    curses.attr_off(last_attr);
                                    curses.attr_on(attr);
                                    last_attr = attr;
                                    let _ = ansi_states.next();
                                }
                                Some(_) | None => {}
                            }
                            self.print_char(curses, ch, if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, is_current)
                        }
                    }
                    idx += 1;
                }
                curses.attr_off(last_attr);

            }

            Some(MatchedRange::Range(start, end)) => {
                // pass
                let item = &matched_item.item;
                let (text, mut idx) = reshape_string(&item.get_text().chars().collect::<Vec<char>>(),
                                                     (self.width-3) as usize,
                                                     self.hscroll_offset,
                                                     start,
                                                     end);
                let mut ansi_states = item.get_ansi_states().iter().peekable();

                // skip ansi states
                let mut last_attr = 0;
                while let Some(&&(index, attr)) = ansi_states.peek() {
                    if idx > index {
                        last_attr = attr;
                        let _ = ansi_states.next();
                    } else {
                        break;
                    }
                }
                curses.attr_on(last_attr);

                for &ch in text.iter() {
                    if idx >= start && idx < end {
                        self.print_char(curses, ch, COLOR_MATCHED, is_current);
                    } else {
                        match ansi_states.peek() {
                            Some(&&(index, attr)) if idx == index => {
                                // print ansi color codes.
                                curses.attr_off(last_attr);
                                curses.attr_on(attr);
                                last_attr = attr;
                                let _ = ansi_states.next();
                            }
                            Some(_) | None => {}
                        }
                        self.print_char(curses, ch, if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, is_current)
                    }
                    idx += 1;
                }
                curses.attr_off(last_attr);
            }

            _ => {
                curses.printw(matched_item.item.get_text());
            }
        }
    }

    fn print_char(&self, curses: &Curses, ch: char, color: i16, is_bold: bool) {
        if ch != '\t' {
            curses.caddch(ch, color, is_bold);
        } else {
            // handle tabstop
            let (_, x) = curses.getyx();
            let rest = (self.tabstop as i32) - (x-2)%(self.tabstop as i32);
            for _ in 0..rest {
                curses.caddch(' ', color, is_bold);
            }
        }
    }

    //--------------------------------------------------------------------------
    // Actions

    pub fn act_move_line_cursor(&mut self, diff: i32) {
        let diff = if self.reverse {-diff} else {diff};
        let mut line_cursor = self.line_cursor as i32;
        let mut item_cursor = self.item_cursor as i32;
        let item_len = self.items.len() as i32;

        line_cursor += diff;
        if line_cursor >= self.height {
            item_cursor += line_cursor - self.height + 1;
            item_cursor = max(0, min(item_cursor, item_len - self.height));
            line_cursor = min(self.height-1, item_len - item_cursor);
        } else if line_cursor < 0 {
            item_cursor += line_cursor;
            item_cursor = max(item_cursor, 0);
            line_cursor = 0;
        } else {
            line_cursor = min(line_cursor, item_len-1 - item_cursor);
        }

        self.item_cursor = item_cursor as usize;
        self.line_cursor = line_cursor as usize;
    }

    pub fn act_toggle(&mut self) {
        if !self.multi_selection {return;}

        let current_item = self.items.get(self.item_cursor + self.line_cursor).unwrap();
        let index = current_item.item.get_full_index();
        if !self.selected.contains_key(&index) {
            self.selected.insert(index, current_item.clone());
        } else {
            self.selected.remove(&index);
        }
    }

    pub fn act_toggle_all(&mut self) {
        for current_item in self.items.iter() {
            let index = current_item.item.get_full_index();
            if !self.selected.contains_key(&index) {
                self.selected.insert(index, current_item.clone());
            } else {
                self.selected.remove(&index);
            }
        }
    }

    pub fn act_select_all(&mut self) {
        for current_item in self.items.iter() {
            let index = current_item.item.get_full_index();
            self.selected.insert(index, current_item.clone());
        }
    }

    pub fn act_deselect_all(&mut self) {
        self.selected.clear();
    }

    pub fn act_output(&mut self) {
        // select the current one
        let current_item = self.items.get(self.item_cursor + self.line_cursor).unwrap();
        let index = current_item.item.get_full_index();
        self.selected.insert(index, current_item.clone());

        let mut output: Vec<_> = self.selected.iter_mut().collect::<Vec<_>>();
        output.sort_by_key(|k| k.0);
        for (_, item) in output {
            println!("{}", item.item.get_output_text());
        }
    }

    pub fn act_scroll(&mut self, offset: i32) {
        let mut hscroll_offset = self.hscroll_offset as i32;
        hscroll_offset += offset;
        hscroll_offset = max(0, hscroll_offset);
        self.hscroll_offset = hscroll_offset as usize;
    }

}

//==============================================================================
// helper functions

// wide character will take two unit
fn display_width(text: &[char]) -> usize {
    text.iter()
        .map(|c| {if c.len_utf8() > 1 {2} else {1}})
        .fold(0, |acc, n| acc + n)
}


// calculate from left to right, stop when the max_x exceeds
fn left_fixed(text: &[char], max_x: usize) -> usize {
    if max_x == 0 {
        return 1;
    }

    let mut w = 0;
    for (idx, &c) in text.iter().enumerate() {
        w += if c.len_utf8() > 1 {2} else {1};
        if w > max_x {
            return idx;
        }
    }
    text.len()
}

fn right_fixed(text: &[char], max_x: usize) -> usize {
    if max_x == 0 {
        return text.len()-1;
    }

    let mut w = 0;
    for (idx, &c) in text.iter().enumerate().rev() {
        w += if c.len_utf8() > 1 {2} else {1};
        if w > max_x {
            return idx+1;
        }
    }
    0
}

// return a string and its left position in original string
// matched_end_pos is char-wise
fn reshape_string(text: &[char],
                  container_width: usize,
                  text_start_pos: usize,
                  matched_start_pos: usize,
                  matched_end_pos: usize) -> (Vec<char>, usize) {
    let text_start_pos = min(max(0, text.len() as i32 - 1) as usize, text_start_pos);
    let full_width = display_width(&text[text_start_pos..]);

    if full_width <= container_width {
        return (text[text_start_pos..].iter().cloned().collect(), text_start_pos);
    }

    let mut ret = Vec::new();

    let w1 = display_width(&text[text_start_pos..matched_start_pos]);
    let w2 = display_width(&text[matched_start_pos..matched_end_pos]);
    let w3 = display_width(&text[matched_end_pos..]);

    let (left_pos, right_pos) = if (w1 > w3 && w2+w3 <= container_width-2) || (w3 <= 2) {
        // right-fixed
        let left_pos = text_start_pos + right_fixed(&text[text_start_pos..], container_width-2);
        (left_pos, text.len())
    } else if w1 <= w3 && w1 + w2 <= container_width-2 {
        // left-fixed
        let right_pos = text_start_pos + left_fixed(&text[text_start_pos..], container_width-2);
        (text_start_pos, right_pos)
    } else {
        // left-right
        let right_pos = max(matched_end_pos, text_start_pos + left_fixed(&text[text_start_pos..], container_width-2));
        let left_pos = text_start_pos + right_fixed(&text[text_start_pos..right_pos], container_width-4);
        (left_pos, right_pos)
    };

    if left_pos > text_start_pos {
        ret.push('.'); ret.push('.');
    }

    // so we should print [left_pos..(right_pos+1)]
    for ch in text[left_pos..right_pos].iter() {
        ret.push(*ch);
    }

    if right_pos < text.len() {
        ret.push('.'); ret.push('.');
    }

    (ret, if left_pos > text_start_pos {left_pos-2} else {left_pos})
}

#[cfg(test)]
mod test {
    #[test]
    fn test_display_width() {
        assert_eq!(super::display_width(&"abcdefg".to_string().chars().collect::<Vec<char>>()), 7);
        assert_eq!(super::display_width(&"This is 中国".to_string().chars().collect::<Vec<char>>()), 12);
    }

    #[test]
    fn test_left_fixed() {
        assert_eq!(super::left_fixed(&"a中cdef".to_string().chars().collect::<Vec<char>>(), 5), 4);
        assert_eq!(super::left_fixed(&"a中".to_string().chars().collect::<Vec<char>>(), 5), 2);
        assert_eq!(super::left_fixed(&"a中".to_string().chars().collect::<Vec<char>>(), 0), 1);
    }

    #[test]
    fn test_right_fixed() {
        assert_eq!(super::right_fixed(&"a中cdef".to_string().chars().collect::<Vec<char>>(), 5), 2);
        assert_eq!(super::right_fixed(&"a中".to_string().chars().collect::<Vec<char>>(), 5), 0);
        assert_eq!(super::right_fixed(&"a中".to_string().chars().collect::<Vec<char>>(), 0), 1);
    }

    #[test]
    fn test_reshape_string() {
        // show all
        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         12, 0, 1, 8),
                   ("0123456789".to_string().chars().collect::<Vec<char>>(), 0));

        // both ellipsis
        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         6, 0, 5, 6),
                   ("..45..".to_string().chars().collect::<Vec<char>>(), 2));

        // left fixed
        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         6, 0, 3, 4),
                   ("0123..".to_string().chars().collect::<Vec<char>>(), 0));

        // right fixed
        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         6, 0, 6, 7),
                   ("..6789".to_string().chars().collect::<Vec<char>>(), 4));

        // right fixed because the remaining is short
        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         6, 0, 1, 8),
                   ("..6789".to_string().chars().collect::<Vec<char>>(), 4));

        // text start pos
        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         6, 2, 3, 5),
                   ("2345..".to_string().chars().collect::<Vec<char>>(), 2));

    }
}

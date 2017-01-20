use std::sync::mpsc::{Receiver, Sender};
use event::{Event, EventArg};
use item::{MatchedItem, MatchedItemGroup, MatchedRange};
use std::time::Instant;
use std::cmp::{max, min};
use orderedvec::OrderedVec;
use std::sync::Arc;
use std::collections::HashMap;

use curses::*;
use curses;
use getopts;

//use std::io::Write;
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
    selected: HashMap<(usize, usize), Arc<MatchedItem>>,

    item_cursor: usize, // the index of matched item currently highlighted.
    line_cursor: usize, // line No.
    hscroll_offset: usize,
    reverse: bool,
    height: i32,
    width: i32,

    multi_selection: bool,
    pub tabstop: usize,
    theme: ColorTheme,

    reader_stopped: bool,
    matcher_stopped: bool,
    num_read: usize,
    num_processed: usize,
    matcher_mode: String,
    timer: Instant,
}

impl Model {
    pub fn new(rx_cmd: Receiver<(Event, EventArg)>) -> Self {
        Model {
            rx_cmd: rx_cmd,
            items: OrderedVec::new(),
            num_read: 0,
            num_processed: 0,
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
            matcher_stopped: false,
            timer: Instant::now(),
            matcher_mode: "".to_string(),
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

        if options.opt_present("reverse") {
            self.reverse = true;
        }

        if let Some(color) = options.opt_str("color") {
            self.theme = ColorTheme::from_options(&color);
        }
    }

    pub fn init(&mut self) {
        curses::init(Some(&self.theme), false, false);
    }

    pub fn run(&mut self, curses: Curses) {
        // generate a new instance of curses for printing


        // main loop
        loop {
            // check for new item
            if let Ok((ev, arg)) = self.rx_cmd.recv() {
                match ev {
                    Event::EvModelNewItem => {
                        let items: MatchedItemGroup = *arg.downcast().unwrap();
                        self.insert_new_items(items);
                    }

                    Event::EvModelRestart => {
                        // clean the model
                        self.clean_model();
                        self.update_size(&curses);
                    }

                    Event::EvModelDrawQuery => {
                        let print_query_func = *arg.downcast::<ClosureType>().unwrap();
                        self.draw_query(&curses, print_query_func);
                        curses.refresh();
                    }
                    Event::EvModelDrawInfo => {
                        self.draw_status(&curses);
                        curses.refresh();
                    }

                    Event::EvModelNotifyProcessed => {
                        let num_processed = *arg.downcast::<usize>().unwrap();
                        self.num_processed = num_processed;

                        if ! self.reader_stopped {
                            // if the reader is still running, the number of processed items equals
                            // to the number of read items
                            self.num_read = num_processed;

                            // update the screen
                            if num_processed & 0xFFF == 0 {
                                self.act_redraw_items_and_status(&curses);
                            }
                        }

                    }

                    Event::EvModelNotifyMatcherMode => {
                        self.matcher_mode = *arg.downcast().unwrap();
                    }

                    Event::EvMatcherStopped => {
                        self.matcher_stopped = true;
                        self.act_redraw_items_and_status(&curses);
                    }

                    Event::EvReaderStopped => {
                        // if reader stopped, the num_read is freezed.
                        self.reader_stopped = true;
                        self.num_read = *arg.downcast().unwrap();
                    }

                    Event::EvReaderStarted => {
                        self.reader_stopped = false;
                        self.num_read = 0;
                    }

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
                    }
                    Event::EvActAbort => {
                        let tx_ack: Sender<bool> = *arg.downcast().unwrap();
                        curses.close();
                        let _ = tx_ack.send(true);
                    }
                    Event::EvActUp => {
                        self.act_move_line_cursor(1);
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActDown => {
                        self.act_move_line_cursor(-1);
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActToggle => {
                        self.act_toggle();
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActToggleDown => {
                        self.act_toggle();
                        self.act_move_line_cursor(-1);
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActToggleUp => {
                        self.act_toggle();
                        self.act_move_line_cursor(1);
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActToggleAll => {
                        self.act_toggle_all();
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActSelectAll => {
                        self.act_select_all();
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActDeselectAll => {
                        self.act_deselect_all();
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActPageDown => {
                        let height = 1-self.height;
                        self.act_move_line_cursor(height);
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActPageUp => {
                        let height = self.height-1;
                        self.act_move_line_cursor(height);
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActScrollLeft => {
                        self.act_scroll(*arg.downcast::<i32>().unwrap_or(Box::new(-1)));
                        self.act_redraw_items_and_status(&curses);
                    }
                    Event::EvActScrollRight => {
                        self.act_scroll(*arg.downcast::<i32>().unwrap_or(Box::new(1)));
                        self.act_redraw_items_and_status(&curses);
                    }

                    Event::EvActRedraw => {
                        let print_query_func = *arg.downcast::<ClosureType>().unwrap();
                        self.act_redarw(&curses, print_query_func);
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
        self.matcher_stopped = false;
        if !self.reader_stopped {
            self.num_processed = 0;
        }
    }

    fn update_size(&mut self, curses: &Curses) {
        // update the (height, width)
        curses.endwin();
        curses.refresh();
        let (h, w) = curses.get_maxyx();
        self.height = h-2;
        self.width = w-2;
    }

    fn insert_new_items(&mut self, items: MatchedItemGroup) {
        for item in items {
            self.items.push(Arc::new(item));
        }
    }

    fn draw_items(&mut self, curses: &Curses) {
        // cursor should be placed on query, so store cursor before printing
        let (y, x) = curses.getyx();

        // clear all lines
        let (h, w) = curses.get_maxyx();
        if self.reverse {
            for l in 2..h {
                curses.mv(l, 0);
                curses.clrtoeol();
            }
        } else {
            for l in 0..(h-2) {
                curses.mv(l, 0);
                curses.clrtoeol();
            }
        }

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
            self.draw_item(curses, &item, l == self.line_cursor);
        }

        // restore cursor
        curses.mv(y, x);
    }

    fn draw_status(&self, curses: &Curses) {
        // cursor should be placed on query, so store cursor before printing
        let (y, x) = curses.getyx();

        curses.mv(if self.reverse {1} else {self.height} , 0);
        curses.clrtoeol();

        // display spinner
        if self.reader_stopped {
            self.print_char(curses, ' ', COLOR_NORMAL, false);
        } else {
            let time = self.timer.elapsed();
            let mills = (time.as_secs()*1000) as u32 + time.subsec_nanos()/1000/1000;
            let index = (mills / SPINNER_DURATION) % (SPINNERS.len() as u32);
            self.print_char(curses, SPINNERS[index as usize], COLOR_SPINNER, true);
        }

        // display matched/total number
        curses.cprint(format!(" {}/{}", self.items.len(), self.num_read).as_ref(), COLOR_INFO, false);

        // display the matcher mode
        if !self.matcher_mode.is_empty() {
            curses.cprint(format!("/{}", &self.matcher_mode).as_ref(), COLOR_INFO, false);
        }

        // display the percentage of the number of processed items
        if self.num_processed < self.num_read {
            curses.cprint(format!(" ({}%) ", self.num_processed*100 / self.num_read).as_ref(), COLOR_INFO, false)
        }

        // selected number
        if self.multi_selection && !self.selected.is_empty() {
            curses.cprint(format!(" [{}]", self.selected.len()).as_ref(), COLOR_INFO, true);
        }

        // item cursor
        let line_num_str = format!(" {} ", self.item_cursor+self.line_cursor);
        curses.mv(if self.reverse {1} else {self.height}, self.width - (line_num_str.len() as i32));
        curses.cprint(&line_num_str, COLOR_INFO, true);

        // restore cursor
        curses.mv(y, x);
    }

    fn draw_query(&self, curses: &Curses, print_query_func: ClosureType) {
        let (h, w) = curses.get_maxyx();
        let (h, _) = (h as usize, w as usize);

        // print query
        curses.mv((if self.reverse {0} else {h-1}) as i32, 0);
        curses.clrtoeol();
        print_query_func(curses);
    }

    fn draw_item(&self, curses: &Curses, matched_item: &MatchedItem, is_current: bool) {
        let index = matched_item.item.get_full_index();

        if self.selected.contains_key(&index) {
            curses.cprint(">", COLOR_SELECTED, true);
        } else {
            curses.cprint(" ", if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, false);
        }

        match matched_item.matched_range {
            Some(MatchedRange::Chars(ref matched_indics)) => {
                let (match_start, match_end) = if !matched_indics.is_empty() {
                    (matched_indics[0], matched_indics[matched_indics.len()-1] + 1)
                } else {
                    (0, 0)
                };

                let item = &matched_item.item;
                let text: Vec<_> = item.get_text().chars().collect();
                let (shift, full_width) = reshape_string(&text, self.width as usize, match_start, match_end, self.tabstop);

                let mut printer = LinePrinter::builder()
                    .tabstop(self.tabstop)
                    .container_width(self.width as usize)
                    .shift(shift)
                    .text_width(full_width)
                    .hscroll_offset(self.hscroll_offset)
                    .build();

                let mut matched_indics_iter = matched_indics.iter().peekable();
                let mut ansi_states = item.get_ansi_states().iter().peekable();

                for (ch_idx, &ch) in text.iter().enumerate() {
                    match matched_indics_iter.peek() {
                        Some(&&match_idx) if ch_idx == match_idx => {
                            printer.print_char(curses, ch, COLOR_MATCHED, is_current);
                            let _ = matched_indics_iter.next();
                        }
                        Some(_) | None => {
                            // print ansi color codes.
                            while let Some(&&(ansi_idx, attr)) = ansi_states.peek() {
                                if ch_idx == ansi_idx {
                                    curses.attr_on(attr);
                                    let _ = ansi_states.next();
                                } else if ch_idx > ansi_idx {
                                    let _ = ansi_states.next();
                                } else {
                                    break;
                                }
                            }
                            printer.print_char(curses, ch, if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, is_current)
                        }
                    }
                }
                curses.attr_on(0);
            }

            Some(MatchedRange::Range(match_start, match_end)) => {
                // pass
                let item = &matched_item.item;
                let text: Vec<_> = item.get_text().chars().collect();
                let (shift, full_width) = reshape_string(&text, self.width as usize, match_start, match_end, self.tabstop);

                let mut printer = LinePrinter::builder()
                    .tabstop(self.tabstop)
                    .container_width(self.width as usize)
                    .shift(shift)
                    .text_width(full_width)
                    .hscroll_offset(self.hscroll_offset)
                    .build();


                let mut ansi_states = item.get_ansi_states().iter().peekable();

                for (ch_idx, &ch) in text.iter().enumerate() {
                    if ch_idx >= match_start && ch_idx < match_end {
                        printer.print_char(curses, ch, COLOR_MATCHED, is_current);
                    } else {
                        // print ansi color codes.
                        while let Some(&&(ansi_idx, attr)) = ansi_states.peek() {
                            if ch_idx == ansi_idx {
                                curses.attr_on(attr);
                                let _ = ansi_states.next();
                            } else if ch_idx > ansi_idx {
                                let _ = ansi_states.next();
                            } else {
                                break;
                            }
                        }
                        printer.print_char(curses, ch, if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, is_current)
                    }
                }
                curses.attr_on(0);
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
            line_cursor = max(0, min(line_cursor, item_len-1 - item_cursor));
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
        if !self.items.is_empty() {
            let current_item = self.items.get(self.item_cursor + self.line_cursor).unwrap();
            let index = current_item.item.get_full_index();
            self.selected.insert(index, current_item.clone());
        }

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

    pub fn act_redarw(&mut self, curses: &Curses, print_query_func: ClosureType) {
        self.update_size(curses);
        curses.erase();
        self.draw_items(curses);
        self.draw_status(curses);
        self.draw_query(curses, print_query_func);
        curses.refresh();
    }

    fn act_redraw_items_and_status(&mut self, curses: &Curses) {
        self.draw_items(curses);
        self.draw_status(curses);
        curses.refresh();
    }

}

struct LinePrinter {
    tabstop: usize,
    start: i32,
    end: i32,
    current: i32,

    shift: usize,
    text_width: usize,
    container_width: usize,
    hscroll_offset: usize,
}

impl LinePrinter {
    pub fn builder() -> Self {
        LinePrinter {
            tabstop: 8,
            start: 0,
            end: 0,
            current: -1,

            shift: 0,
            text_width: 0,
            container_width: 0,
            hscroll_offset: 0,
        }
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
        self.start = (self.shift + self.hscroll_offset) as i32;
        self.end = self.start + self.container_width as i32;
        self
    }


    fn print_char_raw(&mut self, curses: &Curses, ch: char, color: i16, is_bold: bool) {
        // the hidden chracter
        let w = rune_width(ch);

        self.current += w as i32;

        if self.current < self.start {
            // pass if it is hidden
        } else if self.current < self.start + 2 && (self.shift > 0 || self.hscroll_offset > 0) {
            // print left ".."
            for _ in 0..min(w as i32, self.current - self.start + 1) {
                curses.caddch('.', color, is_bold);
            }
        } else if self.current >= self.end {
            // overflow the line
        } else if self.end - self.current <= 2 && (self.text_width as i32 >= self.end) {
            // print right ".."
            for _ in 0..min(w as i32, self.end - self.current) {
                curses.caddch('.', color, is_bold);
            }
        } else {
            curses.caddch(ch, color, is_bold);
        }
    }

    pub fn print_char(&mut self, curses: &Curses, ch: char, color: i16, is_bold: bool) {
        if ch != '\t' {
            self.print_char_raw(curses, ch, color, is_bold);
        } else {
            // handle tabstop
            let rest = if self.current < 0 {
                self.tabstop
            } else {
                self.tabstop - (self.current as usize) % self.tabstop
            };
            for _ in 0..rest {
                self.print_char_raw(curses, ' ', color, is_bold);
            }
        }
    }
}

//==============================================================================
// helper functions

// a very naive solution
// actually only east asian characters occupies 2 characters
fn rune_width(ch: char) -> usize {
    if ch.len_utf8() > 1 {
        2
    } else {
        1
    }
}

// return an array, arr[i] store the display width till char[i]
fn accumulate_text_width(text: &[char], tabstop: usize) -> Vec<usize> {
    let mut ret = Vec::new();
    let mut w = 0;
    for &ch in text.iter() {
        w += if ch == '\t' {
            tabstop - (w % tabstop)
        } else {
            rune_width(ch)
        };
        ret.push(w);
    }
    ret
}

// return (left_shift, full_print_width)
fn reshape_string(text: &[char],
                  container_width: usize,
                  match_start: usize,
                  match_end: usize,
                  tabstop: usize) -> (usize, usize) {

    let acc_width = accumulate_text_width(text, tabstop);
    let full_width = acc_width[acc_width.len()-1];
    if full_width <= container_width {
        return (0, full_width);
    }

    // w1, w2, w3 = len_before_matched, len_matched, len_after_matched
    let w1 = if match_start == 0 {0} else {acc_width[match_start-1]};
    let w2 = if match_end >= text.len() {full_width - w1} else {acc_width[match_end] - w1};
    let w3 = acc_width[acc_width.len()-1] - w1 - w2;

    if (w1 > w3 && w2+w3 <= container_width) || (w3 <= 2) {
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

#[cfg(test)]
mod tests {
    use super::{rune_width, accumulate_text_width, reshape_string};

    fn to_chars(s: &str) -> Vec<char> {
        s.to_string().chars().collect()
    }

    #[test]
    fn test_rune_width() {
        assert_eq!(rune_width('a'), 1);
        assert_eq!(rune_width('中'), 2);
    }

    #[test]
    fn test_accumulate_text_width() {
        assert_eq!(accumulate_text_width(&to_chars(&"abcdefg"), 8), vec![1,2,3,4,5,6,7]);
        assert_eq!(accumulate_text_width(&to_chars(&"ab中de国g"), 8), vec![1,2,4,5,6,8,9]);
        assert_eq!(accumulate_text_width(&to_chars(&"ab\tdefg"), 8), vec![1,2,8,9,10,11,12]);
        assert_eq!(accumulate_text_width(&to_chars(&"ab中\te国g"), 8), vec![1,2,4,8,9,11,12]);
    }

    #[test]
    fn test_reshape_string() {
        // no match, left fixed to 0
        assert_eq!(reshape_string(&to_chars(&"abc"), 10, 0, 0, 8)
                   , (0, 3));
        assert_eq!(reshape_string(&to_chars(&"a\tbc"), 8, 0, 0, 8)
                   , (0, 10));
        assert_eq!(reshape_string(&to_chars(&"a\tb\tc"), 10, 0, 0, 8)
                   , (0, 17));
        assert_eq!(reshape_string(&to_chars(&"a\t中b\tc"), 8, 0, 0, 8)
                   , (0, 17));
        assert_eq!(reshape_string(&to_chars(&"a\t中b\tc012345"), 8, 0, 0, 8)
                   , (0, 23));
    }
}

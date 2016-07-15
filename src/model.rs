/// Model represents the global states needed in FZF.
/// It will also define how the states will be shown on the terminal


use std::sync::{Arc, RwLock, Mutex};
use item::{Item, MatchedItem, MatchedRange};
use ncurses::*;
use std::cmp::{min, max};
use std::collections::HashSet;
use orderedvec::OrderedVec;
use curses::*;
use query::Query;
use util::eventbox::EventBox;
use event::Event;
use std::time::{Instant, Duration};
use std::thread;
use getopts;

// The whole screen is:
//
//                  +---------------------------------------|
//                  | | |                                   | 5
//                  | | |               ^                   | 4
//   current cursor |>| |               |                   | 3
//                  | | |      lines    |                   | 2 cursor
//         selected | |>|--------------------------------   | 1
//                  | | |                                   | 0
//                  +---------------------------------------+
//          spinner |/| | (matched/total) (per%) [selected] |
//                  +---------------------------------------+
//                  | prompt>  query string                 |
//                  +---------------------------------------+
//

const SPINNER_DURATION: u32 = 200;
const REFRESH_DURATION: u64 = 100;
const SPINNERS: [char; 8] = ['-', '\\', '|', '/', '-', '\\', '|', '/'];


pub struct Model {
    eb: Arc<EventBox<Event>>,
    pub query: Query,

    pub items: Arc<RwLock<Vec<Item>>>, // all items
    selected_indics: HashSet<usize>,
    pub matched_items: Arc<RwLock<OrderedVec<MatchedItem>>>,
    num_total: usize,
    percentage: u64,

    pub multi_selection: bool,
    pub prompt: String,
    pub reading: bool,

    item_cursor: usize, // the index of matched item currently highlighted.
    line_cursor: usize, // line No.
    hscroll_offset: usize,

    max_y: i32,
    max_x: i32,
    width: usize,
    height: usize,

    refresh_block: Arc<Mutex<u64>>,
    update_finished: Arc<Mutex<bool>>,

    pub tabstop: usize,
    pub is_interactive: bool,
    curses: Curses,
    timer: Instant,
    accept_key: Option<String>,
}

impl Model {
    pub fn new(eb: Arc<EventBox<Event>>, curses: Curses) -> Self {
        let (max_y, max_x) = curses.get_maxyx();
        let timer = Instant::now();

        Model {
            eb: eb,
            query: Query::new(),
            items: Arc::new(RwLock::new(Vec::new())),
            selected_indics: HashSet::new(),
            matched_items: Arc::new(RwLock::new(OrderedVec::new())),
            num_total: 0,
            percentage: 0,
            multi_selection: false,
            prompt: "> ".to_string(),
            reading: false,
            item_cursor: 0,
            line_cursor: 0,
            hscroll_offset: 0,
            max_y: max_y,
            max_x: max_x,
            width: (max_x - 2) as usize,
            height: (max_y - 2) as usize,
            refresh_block: Arc::new(Mutex::new(0)),
            update_finished: Arc::new(Mutex::new(true)),
            tabstop: 8,
            curses: curses,
            timer: timer,
            accept_key: None,
            is_interactive: false,
        }
    }

    pub fn parse_options(&mut self, options: &getopts::Matches) {
        if options.opt_present("i") {
            self.is_interactive = true;
        }
        if options.opt_present("m") {
            self.multi_selection = true;
        }
        if let Some(prompt) = options.opt_str("p") {
            self.prompt = prompt.clone();
        }
    }

    pub fn clear_items(&self) {
        self.items.write().unwrap().clear();
        self.matched_items.write().unwrap().clear();
    }

    pub fn output(&self) {
        if let Some(ref key) = self.accept_key  { println!("{}", key); }

        let mut selected = self.selected_indics.iter().collect::<Vec<&usize>>();
        selected.sort();
        let items = self.items.read().unwrap();
        for index in selected {
            println!("{}", items[*index].text);
        }
    }

    pub fn update_num_total(&mut self, num_new_items: usize) {
        self.num_total = num_new_items + self.items.read().unwrap().len();
    }

    pub fn update_percentage(&mut self, percentage: u64) {
        self.percentage = percentage;
    }

    pub fn update_matched_items(&mut self, items: Arc<RwLock<OrderedVec<MatchedItem>>>) {
        self.matched_items = items;

        // update cursor
        let item_len = self.matched_items.read().unwrap().len();
        self.item_cursor = min(self.item_cursor, if item_len > 0 {item_len-1} else {0});
        self.line_cursor = min(self.line_cursor, self.item_cursor);
    }

    pub fn print_query(&self) {
        {*self.update_finished.lock().unwrap() = false;}
        // > query
        self.curses.mv(self.max_y-1, 0);
        self.curses.clrtoeol();
        self.curses.cprint(&self.prompt, COLOR_PROMPT, false);
        self.curses.cprint(&self.query.get_query(), COLOR_NORMAL, true);
        self.curses.mv(self.max_y-1, (self.query.pos+self.prompt.len()) as i32);
        {*self.update_finished.lock().unwrap() = true;}
    }

    pub fn print_info(&self) {
        {*self.update_finished.lock().unwrap() = false;}
        let (orig_y, orig_x) = self.curses.get_yx();

        self.curses.mv(self.max_y-2, 0);
        self.curses.clrtoeol();

        if self.reading {
            let time = self.timer.elapsed();
            let mills = (time.as_secs()*1000) as u32 + time.subsec_nanos()/1000/1000;
            let index = (mills / SPINNER_DURATION) % (SPINNERS.len() as u32);
            self.print_char(SPINNERS[index as usize], COLOR_SPINNER, true);
        } else {
            self.print_char(' ', COLOR_NORMAL, false);
        }

        let num_matched = self.matched_items.read().unwrap().len();

        self.curses.cprint(format!(" {}/{}", num_matched, self.num_total).as_ref(), COLOR_INFO, false);

        if self.multi_selection && self.selected_indics.len() > 0 {
            self.curses.cprint(format!(" [{}]", self.selected_indics.len()).as_ref(), COLOR_INFO, true);
        }

        if self.percentage < 100 {
            self.curses.cprint(format!(" ({}%)", self.percentage).as_ref(), COLOR_INFO, false);
        }

        self.curses.mv(orig_y, orig_x);
        {*self.update_finished.lock().unwrap() = true;}
    }

    pub fn print_items(&self) {
        {*self.update_finished.lock().unwrap() = false;}
        let (orig_y, orig_x) = self.curses.get_yx();

        let mut matched_items = self.matched_items.write().unwrap();
        let item_start_pos = self.item_cursor - self.line_cursor;

        for i in 0..self.height {
            self.curses.mv((self.height - i - 1) as i32, 0);
            self.curses.clrtoeol();

            if let Some(matched) = matched_items.get(item_start_pos + i) {
                let is_current_line = i == self.line_cursor;
                let label = if is_current_line {">"} else {" "};
                self.curses.cprint(label, COLOR_CURSOR, true);
                self.print_item(matched, is_current_line);
            } else {
            }
        }

        self.curses.mv(orig_y, orig_x);
        {*self.update_finished.lock().unwrap() = true;}
    }

    fn print_item(&self, matched: &MatchedItem, is_current: bool) {
        let items = self.items.read().unwrap();
        let ref item = items[matched.index];

        let is_selected = self.selected_indics.contains(&matched.index);

        if is_selected {
            self.curses.cprint(">", COLOR_SELECTED, true);
        } else {
            self.curses.cprint(" ", if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, false);
        }

        match matched.matched_range {
            Some(MatchedRange::Chars(ref matched_indics)) => {
                let matched_end_pos = if matched_indics.len() > 0 {
                    matched_indics[matched_indics.len()-1]
                } else {
                    0
                };

                let (text, mut idx) = reshape_string(&item.text.chars().collect::<Vec<char>>(),
                                                     (self.max_x-3) as usize,
                                                     self.hscroll_offset,
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
                self.curses.attr_on(last_attr);

                for &ch in text.iter() {
                    match matched_indics_iter.peek() {
                        Some(&&index) if idx == index => {
                            self.print_char(ch, COLOR_MATCHED, is_current);
                            let _ = matched_indics_iter.next();
                        }
                        Some(_) | None => {
                            match ansi_states.peek() {
                                Some(&&(index, attr)) if idx == index => {
                                    // print ansi color codes.
                                    self.curses.attr_off(last_attr);
                                    self.curses.attr_on(attr);
                                    last_attr = attr;
                                    let _ = ansi_states.next();
                                }
                                Some(_) | None => {}
                            }
                            self.print_char(ch, if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, is_current)
                        }
                    }
                    idx += 1;
                }
                self.curses.attr_off(last_attr);
            }
            Some(MatchedRange::Range(_, _)) => {
                // pass
            }
            None => {
                // pass
            }
        }
    }

    fn print_char(&self, ch: char, color: i16, is_bold: bool) {
        if ch != '\t' {
            self.curses.caddch(ch, color, is_bold);
        } else {
            // handle tabstop
            let mut y = 0;
            let mut x = 0;
            getyx(stdscr, &mut y, &mut x);
            let rest = (self.tabstop as i32) - (x-2)%(self.tabstop as i32);
            for _ in 0..rest {
                self.curses.caddch(' ', color, is_bold);
            }
        }
    }

    pub fn refresh(&self) {
        if *self.update_finished.lock().unwrap() {
            refresh();
        }
    }

    pub fn refresh_throttle(&self) {
        refresh_throttle(self.refresh_block.clone(), self.update_finished.clone());
    }

    pub fn display(&self) {
        self.print_items();
        self.print_info();
        self.print_query();
    }

    // the terminal resizes, so we need to recalculate the margins.
    pub fn resize(&mut self) {
        clear();
        endwin();
        self.refresh();
        let (max_y, max_x) = self.curses.get_maxyx();
        self.max_y  = max_y;
        self.max_x  = max_x;
        self.width  = (max_x - 2) as usize;
        self.height = (max_y - 2) as usize;
    }

    pub fn close(&mut self) {
        self.curses.close();
    }

    //============================================================================
    // Actions

    // return the number selected.
    pub fn act_accept(&mut self, accept_key: Option<String>) -> usize {
        self.accept_key = accept_key;

        let mut matched_items = self.matched_items.write().unwrap();
        if let Some(matched) = matched_items.get(self.item_cursor) {
            let item_index = matched.index;
            self.selected_indics.insert(item_index);
        }
        self.selected_indics.len()
    }

    pub fn act_add_char(&mut self, ch: char) {
        let changed = self.query.add_char(ch);
        if changed {
            self.eb.set(Event::EvQueryChange, Box::new(self.query.get_query()));
        }
    }

    pub fn act_backward_char(&mut self) {
        let _ = self.query.backward_char();
    }

    pub fn act_backward_delete_char(&mut self) {
        let changed = self.query.backward_delete_char();
        if changed {
            self.eb.set(Event::EvQueryChange, Box::new(self.query.get_query()));
        }
    }

    pub fn act_backward_kill_word(&mut self) {
        let changed = self.query.backward_kill_word();
        if changed {
            self.eb.set(Event::EvQueryChange, Box::new(self.query.get_query()));
        }
    }

    pub fn act_backward_word(&mut self) {
        let _ = self.query.backward_word();
    }

    pub fn act_beginning_of_line(&mut self) {
        let _ = self.query.beginning_of_line();
    }

    pub fn act_delete_char(&mut self) {
        let changed = self.query.delete_char();
        if changed {
            self.eb.set(Event::EvQueryChange, Box::new(self.query.get_query()));
        }
    }

    pub fn act_deselect_all(&mut self) {
        self.selected_indics.clear();
    }

    pub fn act_end_of_line(&mut self) {
        let _ = self.query.end_of_line();
    }

    pub fn act_forward_char(&mut self) {
        let _ = self.query.forward_char();
    }

    pub fn act_forward_word(&mut self) {
        let _ = self.query.forward_word();
    }

    pub fn act_kill_line(&mut self) {
        let changed = self.query.kill_line();
        if changed {
            self.eb.set(Event::EvQueryChange, Box::new(self.query.get_query()));
        }
    }

    pub fn act_kill_word(&mut self) {
        let changed = self.query.kill_word();
        if changed {
            self.eb.set(Event::EvQueryChange, Box::new(self.query.get_query()));
        }
    }

    pub fn act_line_discard(&mut self) {
        let changed = self.query.line_discard();
        if changed {
            self.eb.set(Event::EvQueryChange, Box::new(self.query.get_query()));
        }
    }

    pub fn act_select_all(&mut self) {
        if !self.multi_selection {return;}

        let matched_items = self.matched_items.read().unwrap();
        for item in matched_items.iter() {
            self.selected_indics.insert(item.index);
        }
    }

    pub fn act_toggle_all(&mut self) {
        if !self.multi_selection {return;}

        let matched_items = self.matched_items.read().unwrap();
        for item in matched_items.iter() {
            if !self.selected_indics.contains(&item.index) {
                self.selected_indics.insert(item.index);
            } else {
                self.selected_indics.remove(&item.index);
            }
        }
    }

    pub fn act_toggle(&mut self) {
        if !self.multi_selection {return;}

        let mut matched_items = self.matched_items.write().unwrap();
        if let Some(matched) = matched_items.get(self.item_cursor) {
            let item_index = matched.index;
            if self.selected_indics.contains(&item_index) {
                self.selected_indics.remove(&item_index);
            } else {
                self.selected_indics.insert(item_index);
            }
        }
    }

    pub fn act_move_line_cursor(&mut self, diff: i32) {
        let total_item = self.matched_items.read().unwrap().len() as i32;

        let y = self.line_cursor as i32 + diff;
        self.line_cursor = if diff > 0 {
            let tmp = min(min(y, (self.height as i32) -1), total_item-1);
            if tmp < 0 {0} else {tmp as usize}
        } else {
            max(0, y) as usize
        };


        let item_y = self.item_cursor as i32 + diff;
        self.item_cursor = if diff > 0 {
            let tmp = min(item_y, total_item-1);
            if tmp < 0 {0} else {tmp as usize}
        } else {
            max(0, item_y) as usize
        }
    }

    pub fn act_move_page(&mut self, pages: i32) {
        let lines = (self.height as i32) * pages;
        self.act_move_line_cursor(lines);
    }

    pub fn act_vertical_scroll(&mut self, cols: i32) {
        let new_offset = self.hscroll_offset as i32 + cols;
        self.hscroll_offset = max(0, new_offset) as usize;
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
    if max_x <= 0 {
        return 0;
    }

    let mut w = 0;
    for (idx, &c) in text.iter().enumerate() {
        w += if c.len_utf8() > 1 {2} else {1};
        if w > max_x {
            return idx-1;
        }
    }
    return text.len()-1;
}

fn right_fixed(text: &[char], max_x: usize) -> usize {
    if max_x <= 0 {
        return text.len()-1;
    }

    let mut w = 0;
    for (idx, &c) in text.iter().enumerate().rev() {
        w += if c.len_utf8() > 1 {2} else {1};
        if w > max_x {
            return idx+1;
        }
    }
    return 0;

}

// return a string and its left position in original string
// matched_end_pos is char-wise
fn reshape_string(text: &Vec<char>,
                  container_width: usize,
                  text_start_pos: usize,
                  matched_end_pos: usize) -> (Vec<char>, usize) {
    let text_start_pos = min(max(0, text.len() as i32 - 1) as usize, text_start_pos);
    let full_width = display_width(&text[text_start_pos..]);

    if full_width <= container_width {
        return (text[text_start_pos..].iter().map(|x| *x).collect(), text_start_pos);
    }

    let mut ret = Vec::new();
    let mut ret_pos;

    // trim right, so that 'String' -> 'Str..'
    let right_pos = 1 + max(matched_end_pos, text_start_pos + left_fixed(&text[text_start_pos..], container_width-2));
    let mut left_pos = text_start_pos + right_fixed(&text[text_start_pos..right_pos], container_width-2);
    ret_pos = left_pos;

    if left_pos > text_start_pos {
        left_pos = text_start_pos + right_fixed(&text[text_start_pos..right_pos], container_width-4);
        ret.push('.'); ret.push('.');
        ret_pos = left_pos - 2;
    }

    // so we should print [left_pos..(right_pos+1)]
    for ch in text[left_pos..right_pos].iter() {
        ret.push(*ch);
    }
    ret.push('.'); ret.push('.');
    (ret, ret_pos)
}

pub fn refresh_throttle(refresh_block: Arc<Mutex<u64>>, update_finished: Arc<Mutex<bool>>) {
    {
        let mut num_blocks = refresh_block.lock().unwrap();

        *num_blocks += 1;
        if *num_blocks > 1 {
            return;
        }
    }

    if *update_finished.lock().unwrap() {
        refresh();
    }

    let refresh_block = refresh_block.clone();
    let update_finished = update_finished.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(REFRESH_DURATION));
        let num = {
            let mut num_blocks = refresh_block.lock().unwrap();
            let num = *num_blocks;
            *num_blocks = 0;
            num
        };
        if num > 1 {
            refresh_throttle(refresh_block, update_finished);
        }
    });
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
        assert_eq!(super::left_fixed(&"a中cdef".to_string().chars().collect::<Vec<char>>(), 5), 3);
        assert_eq!(super::left_fixed(&"a中".to_string().chars().collect::<Vec<char>>(), 5), 1);
        assert_eq!(super::left_fixed(&"a中".to_string().chars().collect::<Vec<char>>(), 0), 0);
    }

    #[test]
    fn test_right_fixed() {
        assert_eq!(super::right_fixed(&"a中cdef".to_string().chars().collect::<Vec<char>>(), 5), 2);
        assert_eq!(super::right_fixed(&"a中".to_string().chars().collect::<Vec<char>>(), 5), 0);
        assert_eq!(super::right_fixed(&"a中".to_string().chars().collect::<Vec<char>>(), 0), 1);
    }

    #[test]
    fn test_reshape_string() {
        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         6, 1, 7),
                   ("..67..".to_string().chars().collect::<Vec<char>>(), 4));

        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         12, 1, 7),
                   ("123456789".to_string().chars().collect::<Vec<char>>(), 1));

        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         6, 0, 6),
                   ("..56..".to_string().chars().collect::<Vec<char>>(), 3));

        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         8, 0, 4),
                   ("012345..".to_string().chars().collect::<Vec<char>>(), 0));

        assert_eq!(super::reshape_string(&"0123456789".to_string().chars().collect::<Vec<char>>(),
                                         10, 0, 4),
                   ("0123456789".to_string().chars().collect::<Vec<char>>(), 0));
    }



}

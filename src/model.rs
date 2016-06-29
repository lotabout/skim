/// Model represents the global states needed in FZF.
/// It will also define how the states will be shown on the terminal


use std::sync::{Arc, RwLock};
use item::{Item, MatchedItem, MatchedRange};
use ncurses::*;
use std::cmp::{min, max};
use std::cell::RefCell;
use std::collections::HashSet;
use orderedvec::OrderedVec;
use curses::*;
use input::Key;
use query::Query;
use util::eventbox::EventBox;
use event::Event;

pub struct Model {
    eb: Arc<EventBox<Event>>,
    pub query: Query,

    num_matched: u64,
    num_total: u64,
    pub items: Arc<RwLock<Vec<Item>>>, // all items
    selected_indics: HashSet<usize>,
    pub matched_items: RefCell<OrderedVec<MatchedItem>>,
    processed_percentage: u64,

    item_cursor: usize, // the index of matched item currently highlighted.
    line_cursor: usize, // line No.
    item_start_pos: usize, // for screen scroll.
    max_y: i32,
    max_x: i32,

    curses: Curses,
}

impl Model {
    pub fn new(eb: Arc<EventBox<Event>>, curses: Curses) -> Self {
        let (max_y, max_x) = curses.get_maxyx();

        Model {
            eb: eb,
            query: Query::new(),
            num_matched: 0,
            num_total: 0,
            items: Arc::new(RwLock::new(Vec::new())),
            selected_indics: HashSet::new(),
            matched_items: RefCell::new(OrderedVec::new()),
            processed_percentage: 100,
            item_cursor: 0,
            line_cursor: (max_y - 3) as usize,
            item_start_pos: 0,
            max_y: max_y,
            max_x: max_x,
            curses: curses,
        }
    }

    pub fn output(&self) {
        let mut selected = self.selected_indics.iter().collect::<Vec<&usize>>();
        selected.sort();
        let items = self.items.read().unwrap();
        for index in selected {
            println!("{}", items[*index].text);
        }
    }

    pub fn toggle_select(&mut self, selected: Option<bool>) {
        let mut matched_items = self.matched_items.borrow_mut();
        let matched = matched_items.get(self.item_cursor);
        if matched == None {
            return;
        }

        let index = matched.unwrap().index;
        match selected {
            Some(true) => {
                let _ = self.selected_indics.insert(index);
            }
            Some(false) => {
                let _ = self.selected_indics.remove(&index);
            }
            None => {
                if self.selected_indics.contains(&index) {
                    let _ = self.selected_indics.remove(&index);
                } else {
                    let _ = self.selected_indics.insert(index);
                }
            }
        }
    }

    pub fn get_num_selected(&self) -> usize {
        self.selected_indics.len()
    }

    pub fn update_process_info(&mut self, matched: u64, total: u64, processed: u64) {
        self.num_matched = matched;
        self.num_total = total;
        self.processed_percentage = (processed+1)*100/(total+1);
    }

    pub fn push_item(&mut self, item: MatchedItem) {
        self.matched_items.borrow_mut().push(item);
    }

    pub fn clear_items(&mut self) {
        self.matched_items.borrow_mut().clear();
    }

    pub fn move_line_cursor(&mut self, diff: i32) {

        let y = self.line_cursor as i32 + diff;
        let item_y = max(0, self.item_cursor as i32 - diff);
        let screen_height = (self.max_y - 3) as usize;

        match y {
            y if y < 0 => {
                self.line_cursor = 0;
                self.item_cursor = min(item_y as usize, self.matched_items.borrow().len()-1);
                self.item_start_pos = self.item_cursor - screen_height;
            }

            y if y > screen_height as i32 => {
                self.line_cursor = screen_height;
                self.item_cursor = max(0, item_y as usize);
                self.item_start_pos = self.item_cursor;
            }

            y => {
                self.line_cursor = y as usize;
                self.item_cursor = item_y as usize;
            }
        }
    }

    pub fn print_query(&self) {
        // > query
        mv(self.max_y-1, 0);
        addstr("> ");
        addstr(&self.query.get_query());
        mv(self.max_y-1, (self.query.pos+2) as i32);
    }

    pub fn print_info(&self) {
        mv(self.max_y-2, 0);
        addstr(format!("  {}/{}{}", self.num_matched, self.num_total,
                       if self.processed_percentage == 100 {"".to_string()} else {format!("({}%)", self.processed_percentage)}
                       ).as_str());
    }

    fn print_item(&self, matched: &MatchedItem, is_current: bool) {
        let items = self.items.read().unwrap();
        let ref item = items[matched.index];

        let is_selected = self.selected_indics.contains(&matched.index);

        if is_selected {
            self.curses.cprint(COLOR_SELECTED, true, ">");
        } else {
            self.curses.cprint(if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, false, " ");
        }

        match matched.matched_range {
            Some(MatchedRange::Chars(ref matched_indics)) => {
                let matched_end_pos = if matched_indics.len() > 0 {matched_indics[matched_indics.len()-1]} else {item.text.len()-1};
                let (text, mut idx) = reshape_string(&item.text.chars().collect::<Vec<char>>(), (self.max_x-3) as usize, 0, matched_end_pos);
                let mut matched_indics_iter = matched_indics.iter().peekable();

                // skip indics
                while let Some(&&index) = matched_indics_iter.peek() {
                    if idx > index {
                        let _ = matched_indics_iter.next();
                    } else {
                        break;
                    }
                }

                for &ch in text.iter() {
                    if let Some(&&index) = matched_indics_iter.peek() {
                        if idx == index {
                            self.curses.caddch(COLOR_MATCHED, is_current, ch);
                            let _ = matched_indics_iter.next();
                        } else {
                            self.curses.caddch(if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, is_current, ch)
                        }
                    } else {
                        self.curses.caddch(if is_current {COLOR_CURRENT} else {COLOR_NORMAL}, is_current, ch)
                    }
                    idx += 1;
                }
            }
            Some(MatchedRange::Range(_, _)) => {
                // pass
            }
            None => {
                // pass
            }
        }

    }

    pub fn print_items(&self) {
        let mut matched_items = self.matched_items.borrow_mut();

        let mut y = self.max_y - 3;
        let mut i = self.item_start_pos;
        while let Some(matched) = matched_items.get(i) {
            i+=1;

            mv(y, 0);
            let is_current_line = y == self.line_cursor as i32;

            let mut label = if is_current_line {">"} else {" "};

            self.curses.cprint(COLOR_CURSOR, true, label);

            self.print_item(matched, is_current_line);

            y -= 1;
            if y < 0 {
                break;
            }
        }
    }

    pub fn refresh(&self) {
        refresh();
    }

    pub fn display(&self) {
        erase();
        self.print_items();
        self.print_info();
        self.print_query();
    }

    // the terminal resizes, so we need to recalculate the margins.
    pub fn resize(&mut self) {
        clear();
        endwin();
        refresh();
        let mut max_y = 0;
        let mut max_x = 0;
        getmaxyx(stdscr, &mut max_y, &mut max_x);
        self.max_y = max_y;
        self.max_x = max_x;
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

}

//==============================================================================
// helper functions

// wide character will take two unit
fn display_width(text: &[char]) -> usize {
    text.iter()
        .map(|c| {if c.len_utf8() > 1 {2} else {1}})
        .fold(0, |acc, n| acc + n)
}


// calculate from left to right, stop when the width exceeds
fn left_fixed(text: &[char], width: usize) -> usize {
    if width <= 0 {
        return 0;
    }

    let mut w = 0;
    for (idx, &c) in text.iter().enumerate() {
        w += if c.len_utf8() > 1 {2} else {1};
        if w > width {
            return idx-1;
        }
    }
    return text.len()-1;
}

fn right_fixed(text: &[char], width: usize) -> usize {
    if width <= 0 {
        return text.len()-1;
    }

    let mut w = 0;
    for (idx, &c) in text.iter().enumerate().rev() {
        w += if c.len_utf8() > 1 {2} else {1};
        if w > width {
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
    let full_width = display_width(&text[text_start_pos..]);

    if full_width <= container_width {
        return (text[text_start_pos..].iter().map(|x| *x).collect(), text_start_pos);
    }

    let mut ret = Vec::new();
    let mut ret_pos = 0;

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

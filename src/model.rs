/// Model represents the global states needed in FZF.
/// It will also define how the states will be shown on the terminal


use std::sync::{Arc, RwLock};
use item::{Item, MatchedItem, MatchedRange};
use ncurses::*;
use std::cmp;
use std::cell::RefCell;
use std::collections::HashSet;
use orderedvec::OrderedVec;
use curses::*;

pub struct Model {
    pub query: String,
    query_cursor: i32,  // > qu<query_cursor>ery
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
    pub fn new(curses: Curses) -> Self {
        let (max_y, max_x) = curses.get_maxyx();

        Model {
            query: String::new(),
            query_cursor: 0,
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
        let index = matched_items.get(self.item_cursor).unwrap().index;
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

    pub fn update_query(&mut self, query: String, cursor: i32) {
        self.query = query;
        self.query_cursor = cursor;
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
        let item_y = cmp::max(0, self.item_cursor as i32 - diff);
        let screen_height = (self.max_y - 3) as usize;

        match y {
            y if y < 0 => {
                self.line_cursor = 0;
                self.item_cursor = cmp::min(item_y as usize, self.matched_items.borrow().len()-1);
                self.item_start_pos = self.item_cursor - screen_height;
            }

            y if y > screen_height as i32 => {
                self.line_cursor = screen_height;
                self.item_cursor = cmp::max(0, item_y as usize);
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
        addstr(&self.query);
        mv(self.max_y-1, self.query_cursor+2);
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
                let mut matched_indics_iter = matched_indics.iter().peekable();
                for (idx, ch) in item.text.chars().enumerate() {
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
                    if idx >= (self.max_x - 3) as usize {
                        break;
                    }
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
}

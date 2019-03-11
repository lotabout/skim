use crate::event::{Event, EventArg, EventHandler, UpdateScreen};
use crate::item::{Item, MatchedItem, MatchedItemGroup, MatchedRange};
/// Handle the selections of items
use crate::util::reshape_string;
use crate::SkimOptions;
use std::cmp::max;
use std::cmp::min;
use std::collections::HashMap;
use std::sync::Arc;
use tuikit::prelude::*;
use unicode_width::UnicodeWidthChar;
use crate::theme::{ColorTheme, DEFAULT_THEME};
use skiplist::OrderedSkipList;

pub struct Selection {
    items: OrderedSkipList<Arc<MatchedItem>>, // all items
    selected: HashMap<(usize, usize), Arc<MatchedItem>>,

    //
    // |>------ items[items.len()-1]
    // |
    // +======+ screen end
    // |
    // |>------ line_cursor, position from screen start
    // |
    // +======+ item_cursor, screen start
    // |
    // |>------ item[0]
    //
    item_cursor: usize, // the index of matched item currently highlighted.
    line_cursor: usize, // line No.
    hscroll_offset: usize,
    height: usize,

    pub tabstop: usize,

    // Options
    multi_selection: bool,
    reverse: bool,
    no_hscroll: bool,
    theme: Arc<ColorTheme>,
}

impl Selection {
    pub fn new() -> Self {
        Selection {
            items: OrderedSkipList::new(),
            selected: HashMap::new(),
            item_cursor: 0,
            line_cursor: 0,
            hscroll_offset: 0,
            height: 0,
            tabstop: 0,
            multi_selection: false,
            reverse: false,
            no_hscroll: false,
            theme: Arc::new(DEFAULT_THEME),
        }
    }

    pub fn with_options(options: &SkimOptions) -> Self {
        let mut selection = Self::new();
        selection.parse_options(options);
        selection
    }

    fn parse_options(&mut self, options: &SkimOptions) {
        if options.multi {
            self.multi_selection = true;
        }

        if options.reverse {
            self.reverse = true;
        }

        if options.no_hscroll {
            self.no_hscroll = true;
        }

        if let Some(tabstop_str) = options.tabstop {
            let tabstop = tabstop_str.parse::<usize>().unwrap_or(8);
            self.tabstop = max(1, tabstop);
        }
    }

    pub fn theme(mut self, theme: Arc<ColorTheme>) -> Self {
        self.theme = theme;
        self
    }

    pub fn replace_items(&mut self, items: OrderedSkipList<Arc<MatchedItem>>) {
        self.items = items;
        self.item_cursor = min(self.item_cursor, self.items.len());
        self.line_cursor = min(self.line_cursor,self.items.len());
    }

    pub fn act_move_line_cursor(&mut self, diff: i32) {
        let diff = if self.reverse { -diff } else { diff };
        let mut line_cursor = self.line_cursor as i32;
        let mut item_cursor = self.item_cursor as i32;
        let item_len = self.items.len() as i32;

        let height = self.height as i32;

        line_cursor += diff;
        if line_cursor >= height {
            item_cursor += line_cursor - height + 1;
            item_cursor = max(0, min(item_cursor, item_len - height));
            line_cursor = min(height - 1, item_len - item_cursor);
        } else if line_cursor < 0 {
            item_cursor += line_cursor;
            item_cursor = max(item_cursor, 0);
            line_cursor = 0;
        } else {
            line_cursor = max(0, min(line_cursor, item_len - 1 - item_cursor));
        }

        self.item_cursor = item_cursor as usize;
        self.line_cursor = line_cursor as usize;
    }

    pub fn act_toggle(&mut self) {
        if !self.multi_selection || self.items.is_empty() {
            return;
        }

        let cursor = self.item_cursor + self.line_cursor;
        let current_item = self
            .items
            .get(&cursor)
            .unwrap_or_else(|| panic!("model:act_toggle: failed to get item {}", cursor));
        let index = current_item.item.get_full_index();
        if !self.selected.contains_key(&index) {
            self.selected.insert(index, Arc::clone(current_item));
        } else {
            self.selected.remove(&index);
        }
    }

    pub fn act_toggle_all(&mut self) {
        for current_item in self.items.iter() {
            let index = current_item.item.get_full_index();
            if !self.selected.contains_key(&index) {
                self.selected.insert(index, Arc::clone(current_item));
            } else {
                self.selected.remove(&index);
            }
        }
    }

    pub fn act_select_all(&mut self) {
        for current_item in self.items.iter() {
            let index = current_item.item.get_full_index();
            self.selected.insert(index, Arc::clone(current_item));
        }
    }

    pub fn act_deselect_all(&mut self) {
        self.selected.clear();
    }

    pub fn act_output(&mut self) {
        // select the current one
        if !self.items.is_empty() {
            let cursor = self.item_cursor + self.line_cursor;
            let current_item = self
                .items
                .get(&cursor)
                .unwrap_or_else(|| panic!("model:act_output: failed to get item {}", cursor));
            let index = current_item.item.get_full_index();
            self.selected.insert(index, Arc::clone(current_item));
        }
    }

    pub fn act_scroll(&mut self, offset: i32) {
        let mut hscroll_offset = self.hscroll_offset as i32;
        hscroll_offset += offset;
        hscroll_offset = max(0, hscroll_offset);
        self.hscroll_offset = hscroll_offset as usize;
    }
}

impl EventHandler for Selection {
    fn accept_event(&self, event: Event) -> bool {
        use crate::event::Event::*;
        match event {
            EvActUp | EvActDown | EvActToggle | EvActToggleDown | EvActToggleUp | EvActToggleAll | EvActSelectAll
            | EvActDeselectAll | EvActPageDown | EvActPageUp | EvActScrollLeft | EvActScrollRight => true,
            _ => false,
        }
    }

    fn handle(&mut self, event: Event, arg: EventArg) -> UpdateScreen {
        use crate::event::Event::*;
        match event {
            EvActUp => {
                self.act_move_line_cursor(1);
            }
            EvActDown => {
                self.act_move_line_cursor(-1);
            }
            EvActToggle => {
                self.act_toggle();
            }
            EvActToggleDown => {
                self.act_toggle();
                self.act_move_line_cursor(-1);
            }
            EvActToggleUp => {
                self.act_toggle();
                self.act_move_line_cursor(1);
            }
            EvActToggleAll => {
                self.act_toggle_all();
            }
            EvActSelectAll => {
                self.act_select_all();
            }
            EvActDeselectAll => {
                self.act_deselect_all();
            }
            EvActPageDown => {
                let height = 1 - (self.height as i32);
                self.act_move_line_cursor(height);
            }
            EvActPageUp => {
                let height = (self.height as i32) - 1;
                self.act_move_line_cursor(height);
            }
            EvActScrollLeft => {
                self.act_scroll(*arg.downcast::<i32>().unwrap_or_else(|_| Box::new(-1)));
            }
            EvActScrollRight => {
                self.act_scroll(*arg.downcast::<i32>().unwrap_or_else(|_| Box::new(1)));
            }
            _ => {}
        }
        UpdateScreen::Redraw
    }
}

impl Selection {
    fn draw_item(&self, canvas: &mut Canvas, row: usize, matched_item: &MatchedItem, is_current: bool) -> Result<()> {
        let (screen_width, _) = canvas.size()?;
        if screen_width < 3 {
            return Err("screen width is too small".into());
        }

        let index = matched_item.item.get_full_index();

        let default_attr = if is_current {
            self.theme.current()
        } else {
            self.theme.normal()
        };

        // print selection cursor
        if self.selected.contains_key(&index) {
            canvas.print_with_attr(row, 0, ">", default_attr.extend(self.theme.selected()));
        } else {
            canvas.print_with_attr(row, 0, " ", default_attr);
        }

        let item = &matched_item.item;
        let text = item.get_text();
        let (match_start_char, match_end_char) = match matched_item.matched_range {
            Some(MatchedRange::Chars(ref matched_indices)) => {
                if !matched_indices.is_empty() {
                    (matched_indices[0], matched_indices[matched_indices.len() - 1] + 1)
                } else {
                    (0, 0)
                }
            }
            Some(MatchedRange::ByteRange(match_start, match_end)) => {
                let match_start_char = text[..match_start].chars().count();
                let diff = text[match_start..match_end].chars().count();
                (match_start_char, match_start_char + diff)
            }
            None => (0, 0),
        };

        let container_width = screen_width - 2;
        let (shift, full_width) =
            reshape_string(&text, container_width, match_start_char, match_end_char, self.tabstop);

        let mut printer = LinePrinter::builder()
            .row(row)
            .col(2)
            .tabstop(self.tabstop)
            .container_width(container_width)
            .shift(if self.no_hscroll { 0 } else { shift })
            .text_width(full_width)
            .hscroll_offset(self.hscroll_offset)
            .build();

        // print out the original content
        if item.get_text_struct().is_some() && item.get_text_struct().as_ref().unwrap().has_attrs() {
            for (ch, attr) in item.get_text_struct().as_ref().unwrap().iter() {
                printer.print_char(canvas, ch, default_attr.extend(attr), false);
            }
        } else {
            for ch in item.get_text().chars() {
                printer.print_char(canvas, ch, default_attr, false);
            }
        }

        // print the highlighted content
        printer.reset();
        match matched_item.matched_range {
            Some(MatchedRange::Chars(ref matched_indices)) => {
                let mut matched_indices_iter = matched_indices.iter().peekable();

                for (ch_idx, ch) in text.chars().enumerate() {
                    match matched_indices_iter.peek() {
                        Some(&&match_idx) if ch_idx == match_idx => {
                            printer.print_char(canvas, ch, default_attr.extend(self.theme.matched()), false);
                            let _ = matched_indices_iter.next();
                        }
                        Some(_) | None => {
                            printer.print_char(canvas, ch, default_attr, true);
                        }
                    }
                }
            }

            Some(MatchedRange::ByteRange(start, end)) => {
                for (idx, ch) in text.char_indices() {
                    printer.print_char(
                        canvas,
                        ch,
                        default_attr.extend(self.theme.matched()),
                        !(idx >= start && idx < end),
                    );
                }
            }

            _ => {}
        }

        Ok(())
    }
}

impl Draw for Selection {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        let (screen_width, screen_height) = canvas.size()?;

        canvas.clear()?;

        let item_idx_lower = self.item_cursor;
        let max_upper = self.item_cursor + screen_height;
        let item_idx_upper = min(max_upper, self.items.len());

        for item_idx in item_idx_lower..item_idx_upper {
            let line_no = if self.reverse {
                // top down
                item_idx - item_idx_lower
            } else {
                // bottom up
                screen_height + item_idx_lower - item_idx
            };

            // print the cursor label
            let label = if line_no == self.line_cursor { ">" } else { " " };
            let next_col = canvas.print_with_attr(line_no, 0, label, self.theme.cursor())?;

            let item = self
                .items
                .get(&item_idx)
                .unwrap_or_else(|| panic!("model:draw_items: failed to get item at {}", item_idx));

            self.draw_item(canvas, line_no, &item, line_no == self.line_cursor);
        }

        Ok(())
    }
}

// use to print a single line, properly handle the tabsteop and shift of a string
// e.g. a long line will be printed as `..some content` or `some content..` or `..some content..`
// depends on the container's width and the size of the content.
//
// let's say we have a very long line with lots of useless information
//                                |.. with lots of use..|             // only to show this
//                                |<- container width ->|
//             |<-    shift    -> |
// |< hscroll >|

struct LinePrinter {
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

    fn print_ch_to_canvas(&mut self, canvas: &mut Canvas, ch: char, attr: Attr, skip: bool) {
        let w = ch.width().unwrap_or(2);

        if !skip {
            let _ = canvas.put_cell(self.row, self.screen_col, Cell::default().ch(ch).attribute(attr));
        }

        self.screen_col += w;
    }

    fn print_char_raw(&mut self, canvas: &mut Canvas, ch: char, attr: Attr, skip: bool) {
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

    pub fn print_char(&mut self, canvas: &mut Canvas, ch: char, attr: Attr, skip: bool) {
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

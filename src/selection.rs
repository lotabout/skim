use std::cmp::max;
use std::cmp::min;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tuikit::prelude::{Event as TermEvent, *};

///! Handle the selections of items
use crate::event::{Event, EventHandler, UpdateScreen};
use crate::global::current_run_num;
use crate::item::MatchedItem;
use crate::orderedvec::OrderedVec;
use crate::theme::{ColorTheme, DEFAULT_THEME};
use crate::util::clear_canvas;
use crate::util::{print_item, reshape_string, LinePrinter};
use crate::{DisplayContext, MatchRange, Matches, Selector, SkimItem, SkimOptions};
use regex::Regex;
use std::rc::Rc;
use unicode_width::UnicodeWidthStr;
use linked_hash_map::LinkedHashMap;

type ItemIndex = (u32, u32);

pub struct Selection {
    // all items
    items: OrderedVec<MatchedItem>,
    selected: LinkedHashMap<ItemIndex, Arc<dyn SkimItem>>,

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
    // the index of matched item currently highlighted.
    item_cursor: usize,
    // line No.
    line_cursor: usize,
    hscroll_offset: i64,
    keep_right: bool,
    skip_to_pattern: Option<Regex>,
    height: AtomicUsize,
    tabstop: usize,

    // Options
    multi_selection: bool,
    reverse: bool,
    no_hscroll: bool,
    theme: Arc<ColorTheme>,

    // Pre-selection will be performed the first time an item was seen by Selection.
    // To avoid remember all items, we'll track the latest run_num and index.
    latest_select_run_num: u32,
    pre_selected_watermark: usize,
    selector: Option<Rc<dyn Selector>>,
}

impl Selection {
    pub fn new() -> Self {
        Selection {
            items: OrderedVec::new(),
            selected: LinkedHashMap::new(),
            item_cursor: 0,
            line_cursor: 0,
            hscroll_offset: 0,
            keep_right: false,
            skip_to_pattern: None,
            height: AtomicUsize::new(0),
            tabstop: 8,
            multi_selection: false,
            reverse: false,
            no_hscroll: false,
            theme: Arc::new(*DEFAULT_THEME),
            latest_select_run_num: 0,
            pre_selected_watermark: 0,
            selector: None,
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

        if options.layout.starts_with("reverse") {
            self.reverse = true;
        }

        if options.no_hscroll {
            self.no_hscroll = true;
        }

        if let Some(tabstop_str) = options.tabstop {
            let tabstop = tabstop_str.parse::<usize>().unwrap_or(8);
            self.tabstop = max(1, tabstop);
        }

        if options.tac {
            self.items.tac(true);
        }

        if options.nosort {
            self.items.nosort(true);
        }

        if !options.skip_to_pattern.is_empty() {
            self.skip_to_pattern = Regex::new(options.skip_to_pattern).ok();
        }

        self.keep_right = options.keep_right;
        self.selector = options.selector.clone();
    }

    pub fn theme(mut self, theme: Arc<ColorTheme>) -> Self {
        self.theme = theme;
        self
    }

    pub fn append_sorted_items(&mut self, items: Vec<MatchedItem>) {
        debug!("append_sorted_items: num: {}", items.len());
        let current_run_num = current_run_num();
        if !items.is_empty() && current_run_num > self.latest_select_run_num {
            self.latest_select_run_num = current_run_num;
            self.pre_selected_watermark = 0;
        }

        if self.items.len() >= self.pre_selected_watermark {
            self.pre_select(&items);
        }

        self.items.append(items);
        self.pre_selected_watermark = max(self.pre_selected_watermark, self.items.len());

        let height = self.height.load(Ordering::Relaxed);
        if self.items.len() <= self.line_cursor {
            // if not enough items, move cursor down
            self.line_cursor = max(min(self.items.len(), height), 1) - 1;
        }

        if self.items.len() <= self.line_cursor + self.item_cursor {
            // if not enough items, scroll the cursor a page down
            self.item_cursor = max(self.items.len(), height) - height;
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    fn pre_select(&mut self, items: &[MatchedItem]) {
        debug!("perform pre selection for {} items", items.len());
        if self.selector.is_none() || !self.multi_selection {
            return;
        }

        let current_run_num = current_run_num();
        for item in items {
            if self
                .selector
                .as_ref()
                .map(|s| s.should_select(item.item_idx as usize, item.item.as_ref()))
                .unwrap_or(false)
            {
                self.act_select_raw_item(current_run_num, item.item_idx, item.item.clone());
            }
        }
        debug!("done perform pre selection for {} items", items.len());
    }

    // > 0 means move up, < 0 means move down
    pub fn act_move_line_cursor(&mut self, diff: i32) {
        let diff = if self.reverse { -diff } else { diff };

        let mut line_cursor = self.line_cursor as i32;
        let mut item_cursor = self.item_cursor as i32;
        let item_len = self.items.len() as i32;

        let height = self.height.load(Ordering::Relaxed) as i32;

        line_cursor += diff;
        if line_cursor >= height {
            item_cursor += line_cursor - height + 1;
            item_cursor = max(0, min(item_cursor, item_len - height));
            line_cursor = min(height - 1, item_len - item_cursor - 1);
        } else if line_cursor < 0 {
            item_cursor += line_cursor;
            item_cursor = max(item_cursor, 0);
            line_cursor = 0;
        } else {
            line_cursor = min(line_cursor, item_len - 1 - item_cursor);
        }

        line_cursor = max(0, line_cursor);

        self.item_cursor = item_cursor as usize;
        self.line_cursor = line_cursor as usize;
    }

    pub fn act_select_screen_row(&mut self, rows_to_top: usize) {
        let height = self.height.load(Ordering::Relaxed);
        let diff = if self.reverse {
            self.line_cursor as i32 - rows_to_top as i32
        } else {
            height as i32 - rows_to_top as i32 - 1 - self.line_cursor as i32
        };
        self.act_move_line_cursor(diff);
    }

    #[allow(clippy::map_entry)]
    pub fn act_toggle(&mut self) {
        if !self.multi_selection || self.items.is_empty() {
            return;
        }

        let cursor = self.item_cursor + self.line_cursor;
        let current_item = self
            .items
            .get(cursor)
            .unwrap_or_else(|| panic!("model:act_toggle: failed to get item {}", cursor));
        let index = (current_run_num(), current_item.item_idx);
        if !self.selected.contains_key(&index) {
            self.selected.insert(index, current_item.item.clone());
        } else {
            self.selected.remove(&index);
        }
    }

    #[allow(clippy::map_entry)]
    pub fn act_toggle_all(&mut self) {
        if !self.multi_selection || self.items.is_empty() {
            return;
        }

        let run_num = current_run_num();
        for current_item in self.items.iter() {
            let index = (run_num, current_item.item_idx);
            if !self.selected.contains_key(&index) {
                self.selected.insert(index, current_item.item.clone());
            } else {
                self.selected.remove(&index);
            }
        }
    }

    pub fn act_select_matched(&mut self, run_num: u32, matched: MatchedItem) {
        self.act_select_raw_item(run_num, matched.item_idx, matched.item.clone());
    }

    pub fn act_select_raw_item(&mut self, run_num: u32, item_index: u32, item: Arc<dyn SkimItem>) {
        if !self.multi_selection {
            return;
        }
        self.selected.insert((run_num, item_index), item);
    }

    pub fn act_select_all(&mut self) {
        if !self.multi_selection || self.items.is_empty() {
            return;
        }

        let run_num = current_run_num();
        for current_item in self.items.iter() {
            let item = current_item.item.clone();
            self.selected.insert((run_num, current_item.item_idx), item);
        }
    }

    pub fn act_deselect_all(&mut self) {
        self.selected.clear();
    }

    pub fn act_scroll(&mut self, offset: i32) {
        self.hscroll_offset += offset as i64;
    }

    pub fn get_selected_indices_and_items(&self) -> (Vec<usize>, Vec<Arc<dyn SkimItem>>) {
        // select the current one
        let select_cursor = !self.multi_selection || self.selected.is_empty();
        let mut selected: Vec<Arc<dyn SkimItem>> = self.selected.values().cloned().collect();
        let mut item_indices: Vec<usize> = self.selected.keys().map(|(_run, idx)| *idx as usize).collect();

        if select_cursor && !self.items.is_empty() {
            let cursor = self.item_cursor + self.line_cursor;
            let current_item = self
                .items
                .get(cursor)
                .unwrap_or_else(|| panic!("model:act_output: failed to get item {}", cursor));
            let item = current_item.item.clone();
            item_indices.push(cursor);
            selected.push(item);
        }

        (item_indices, selected)
    }

    pub fn get_num_of_selected_exclude_current(&self) -> usize {
        self.selected.len()
    }

    pub fn get_current_item_idx(&self) -> usize {
        self.item_cursor + self.line_cursor
    }

    pub fn get_num_selected(&self) -> usize {
        self.selected.len()
    }

    pub fn is_multi_selection(&self) -> bool {
        self.multi_selection
    }

    pub fn get_current_item(&self) -> Option<Arc<dyn SkimItem>> {
        let item_idx = self.get_current_item_idx();
        self.items.get(item_idx).map(|item| item.item.clone())
    }

    pub fn get_hscroll_offset(&self) -> i64 {
        self.hscroll_offset
    }

    pub fn get_num_options(&self) -> usize {
        self.items.len()
    }

    fn calc_skip_width(&self, text: &str) -> usize {
        let skip = if self.skip_to_pattern.is_none() {
            0
        } else {
            let regex = self.skip_to_pattern.as_ref().unwrap();
            if let Some(mat) = regex.find(text) {
                text[..mat.start()].width_cjk()
            } else {
                0
            }
        };
        max(2, skip) - 2
    }
}

impl EventHandler for Selection {
    fn handle(&mut self, event: &Event) -> UpdateScreen {
        use crate::event::Event::*;
        match event {
            EvActUp(diff) => {
                self.act_move_line_cursor(*diff);
            }
            EvActDown(diff) => {
                self.act_move_line_cursor(-*diff);
            }
            EvActToggle => {
                self.act_toggle();
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
            EvActHalfPageDown(diff) => {
                let height = 1 - (self.height.load(Ordering::Relaxed) as i32);
                self.act_move_line_cursor(height * *diff / 2);
            }
            EvActHalfPageUp(diff) => {
                let height = (self.height.load(Ordering::Relaxed) as i32) - 1;
                self.act_move_line_cursor(height * *diff / 2);
            }
            EvActPageDown(diff) => {
                let height = 1 - (self.height.load(Ordering::Relaxed) as i32);
                self.act_move_line_cursor(height * *diff);
            }
            EvActPageUp(diff) => {
                let height = (self.height.load(Ordering::Relaxed) as i32) - 1;
                self.act_move_line_cursor(height * *diff);
            }
            EvActSelectRow(row) => {
                self.act_select_screen_row(*row);
            }
            EvActScrollLeft(diff) => {
                self.act_scroll(-*diff);
            }
            EvActScrollRight(diff) => {
                self.act_scroll(*diff);
            }
            _ => return UpdateScreen::DONT_REDRAW,
        }
        UpdateScreen::REDRAW
    }
}

impl Selection {
    fn draw_item(
        &self,
        canvas: &mut dyn Canvas,
        row: usize,
        matched_item: &MatchedItem,
        is_current: bool,
    ) -> DrawResult<()> {
        let (screen_width, screen_height) = canvas.size()?;

        // update item heights
        self.height.store(screen_height, Ordering::Relaxed);

        if screen_width < 3 {
            return Err("screen width is too small".into());
        }

        let default_attr = if is_current {
            self.theme.current()
        } else {
            self.theme.normal()
        };

        let matched_attr = if is_current {
            self.theme.current_match()
        } else {
            self.theme.matched()
        };

        // print selection cursor
        let index = (current_run_num(), matched_item.item_idx);
        if self.selected.contains_key(&index) {
            let _ = canvas.print_with_attr(row, 1, ">", default_attr.extend(self.theme.selected()));
        } else {
            let _ = canvas.print_with_attr(row, 1, " ", default_attr);
        }

        let item = &matched_item.item;
        let item_text = item.text();
        let container_width = screen_width - 2;

        let matches = match matched_item.matched_range {
            Some(MatchRange::Chars(ref matched_indices)) => Matches::CharIndices(matched_indices),
            Some(MatchRange::ByteRange(start, end)) => Matches::ByteRange(start, end),
            _ => Matches::None,
        };

        let context = DisplayContext {
            text: &item_text,
            score: 0,
            matches,
            container_width,
            highlight_attr: matched_attr,
        };

        let display_content = item.display(context);

        let mut printer = if display_content.stripped() == item_text {
            // need to display the match content
            let (match_start_char, match_end_char) = match matched_item.matched_range {
                Some(MatchRange::Chars(ref matched_indices)) => {
                    if !matched_indices.is_empty() {
                        (matched_indices[0], matched_indices[matched_indices.len() - 1] + 1)
                    } else {
                        (0, 0)
                    }
                }
                Some(MatchRange::ByteRange(match_start, match_end)) => {
                    let match_start_char = item_text[..match_start].chars().count();
                    let diff = item_text[match_start..match_end].chars().count();
                    (match_start_char, match_start_char + diff)
                }
                None => (0, 0),
            };

            let (shift, full_width) = reshape_string(
                &item_text,
                container_width,
                match_start_char,
                match_end_char,
                self.tabstop,
            );

            let shift = if self.no_hscroll {
                0
            } else if match_start_char == 0 && match_end_char == 0 {
                // no match
                if self.keep_right {
                    max(full_width, container_width) - container_width
                } else {
                    self.calc_skip_width(&item_text)
                }
            } else {
                shift
            };

            LinePrinter::builder()
                .row(row)
                .col(2)
                .tabstop(self.tabstop)
                .container_width(container_width)
                .shift(shift)
                .text_width(full_width)
                .hscroll_offset(self.hscroll_offset)
                .build()
        } else {
            LinePrinter::builder()
                .row(row)
                .col(2)
                .tabstop(self.tabstop)
                .container_width(container_width)
                .text_width(display_content.stripped().width_cjk())
                .hscroll_offset(self.hscroll_offset)
                .build()
        };

        // print out the original content
        print_item(canvas, &mut printer, display_content, default_attr);

        Ok(())
    }
}

impl Draw for Selection {
    fn draw(&self, canvas: &mut dyn Canvas) -> DrawResult<()> {
        let (_screen_width, screen_height) = canvas.size()?;
        canvas.clear()?;

        let item_idx_lower = self.item_cursor;
        let max_upper = self.item_cursor + screen_height;
        let item_idx_upper = min(max_upper, self.items.len());

        clear_canvas(canvas)?;

        for item_idx in item_idx_lower..item_idx_upper {
            let line_cursor = item_idx - item_idx_lower;
            let line_no = if self.reverse {
                // top down
                line_cursor
            } else {
                // bottom up
                screen_height - 1 - line_cursor
            };

            // print the cursor label
            let label = if line_cursor == self.line_cursor { ">" } else { " " };
            let _next_col = canvas.print_with_attr(line_no, 0, label, self.theme.cursor()).unwrap();

            let item = self
                .items
                .get(item_idx)
                .unwrap_or_else(|| panic!("model:draw_items: failed to get item at {}", item_idx));

            let _ = self.draw_item(canvas, line_no, &item, line_cursor == self.line_cursor);
        }

        Ok(())
    }
}

impl Widget<Event> for Selection {
    fn on_event(&self, event: TermEvent, _rect: Rectangle) -> Vec<Event> {
        let mut ret = vec![];
        match event {
            TermEvent::Key(Key::WheelUp(.., count)) => ret.push(Event::EvActUp(count as i32)),
            TermEvent::Key(Key::WheelDown(.., count)) => ret.push(Event::EvActDown(count as i32)),
            TermEvent::Key(Key::SingleClick(MouseButton::Left, row, _)) => {
                ret.push(Event::EvActSelectRow(row as usize))
            }
            TermEvent::Key(Key::DoubleClick(MouseButton::Left, ..)) => ret.push(Event::EvActAccept(None)),
            TermEvent::Key(Key::SingleClick(MouseButton::Right, row, _)) => {
                ret.push(Event::EvActSelectRow(row as usize));
                ret.push(Event::EvActToggle);
            }
            _ => {}
        }
        ret
    }
}

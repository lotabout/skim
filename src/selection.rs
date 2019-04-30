///! Handle the selections of items
use crate::event::{Event, EventArg, EventHandler, UpdateScreen};
use crate::item::{parse_criteria, RankCriteria};
use crate::item::{Item, MatchedItem, MatchedRange};
use crate::orderedvec::CompareFunction;
use crate::orderedvec::OrderedVec;
use crate::theme::{ColorTheme, DEFAULT_THEME};
use crate::util::{reshape_string, LinePrinter};
use crate::SkimOptions;
use std::cmp::max;
use std::cmp::min;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tuikit::prelude::*;

lazy_static! {
    static ref DEFAULT_CRITERION: Vec<RankCriteria> = vec![
        RankCriteria::Score,
        RankCriteria::Begin,
        RankCriteria::End,
        RankCriteria::Index,
    ];
}

pub struct Selection {
    criterion: Vec<RankCriteria>,
    items: OrderedVec<MatchedItem>, // all items
    selected: HashMap<(usize, usize), MatchedItem>,

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
    height: AtomicUsize,
    tabstop: usize,

    // Options
    multi_selection: bool,
    reverse: bool,
    no_hscroll: bool,
    theme: Arc<ColorTheme>,
}

impl Selection {
    pub fn new() -> Self {
        Selection {
            criterion: DEFAULT_CRITERION.clone(),
            items: OrderedVec::new(build_compare_function(DEFAULT_CRITERION.clone())),
            selected: HashMap::new(),
            item_cursor: 0,
            line_cursor: 0,
            hscroll_offset: 0,
            height: AtomicUsize::new(0),
            tabstop: 0,
            multi_selection: false,
            reverse: false,
            no_hscroll: false,
            theme: Arc::new(*DEFAULT_THEME),
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

        if let Some(ref tie_breaker) = options.tiebreak {
            let criterion = tie_breaker.split(',').filter_map(parse_criteria).collect();
            self.criterion = criterion;
        }

        if options.tac {
            let criterion = self
                .criterion
                .iter()
                .map(|&criteria| match criteria {
                    RankCriteria::Index => RankCriteria::NegIndex,
                    RankCriteria::NegIndex => RankCriteria::Index,
                    criteria => criteria,
                })
                .collect();
            self.criterion = criterion;
        }

        self.items = OrderedVec::new(build_compare_function(self.criterion.clone()));
    }

    pub fn theme(mut self, theme: Arc<ColorTheme>) -> Self {
        self.theme = theme;
        self
    }

    pub fn append_sorted_items(&mut self, items: Vec<MatchedItem>) {
        self.items.append_ordered(items);

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
            line_cursor = min(height - 1, item_len - item_cursor);
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

    pub fn act_toggle(&mut self) {
        if !self.multi_selection || self.items.is_empty() {
            return;
        }

        let cursor = self.item_cursor + self.line_cursor;
        let current_item = self
            .items
            .get(cursor)
            .unwrap_or_else(|| panic!("model:act_toggle: failed to get item {}", cursor));
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

    pub fn act_scroll(&mut self, offset: i32) {
        let mut hscroll_offset = self.hscroll_offset as i32;
        hscroll_offset += offset;
        hscroll_offset = max(0, hscroll_offset);
        self.hscroll_offset = hscroll_offset as usize;
    }

    pub fn get_selected_items(&mut self) -> Vec<Arc<Item>> {
        // select the current one
        if !self.items.is_empty() {
            let cursor = self.item_cursor + self.line_cursor;
            let current_item = self
                .items
                .get(cursor)
                .unwrap_or_else(|| panic!("model:act_output: failed to get item {}", cursor));
            let index = current_item.item.get_full_index();
            self.selected.insert(index, current_item.clone());
        }

        let mut selected: Vec<Arc<Item>> = self.selected.values().map(|item| item.item.clone()).collect();

        selected.sort_by_key(|item| item.get_full_index());
        selected
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

    pub fn get_current_item(&self) -> Option<Arc<Item>> {
        let item_idx = self.get_current_item_idx();
        self.items.get(item_idx).map(|item| item.item.clone())
    }
}

impl EventHandler for Selection {
    fn accept_event(&self, event: Event) -> bool {
        use crate::event::Event::*;
        match event {
            EvActUp | EvActDown | EvActToggle | EvActToggleAll | EvActSelectAll | EvActDeselectAll | EvActPageDown
            | EvActPageUp | EvActScrollLeft | EvActScrollRight => true,
            _ => false,
        }
    }

    fn handle(&mut self, event: Event, arg: &EventArg) -> UpdateScreen {
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
                let height = 1 - (self.height.load(Ordering::Relaxed) as i32);
                self.act_move_line_cursor(height);
            }
            EvActPageUp => {
                let height = (self.height.load(Ordering::Relaxed) as i32) - 1;
                self.act_move_line_cursor(height);
            }
            EvActScrollLeft => {
                self.act_scroll(*arg.downcast_ref::<i32>().unwrap_or(&-1));
            }
            EvActScrollRight => {
                self.act_scroll(*arg.downcast_ref::<i32>().unwrap_or(&1));
            }
            _ => {}
        }
        UpdateScreen::REDRAW
    }
}

impl Selection {
    fn draw_item(&self, canvas: &mut Canvas, row: usize, matched_item: &MatchedItem, is_current: bool) -> Result<()> {
        let (screen_width, screen_height) = canvas.size()?;

        // update item heights
        self.height.store(screen_height, Ordering::Relaxed);

        if screen_width < 3 {
            return Err("screen width is too small".into());
        }

        let index = matched_item.item.get_full_index();

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
        if self.selected.contains_key(&index) {
            let _ = canvas.print_with_attr(row, 1, ">", default_attr.extend(self.theme.selected()));
        } else {
            let _ = canvas.print_with_attr(row, 1, " ", default_attr);
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
                            printer.print_char(canvas, ch, matched_attr, false);
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
                    printer.print_char(canvas, ch, matched_attr, !(idx >= start && idx < end));
                }
            }

            _ => {}
        }

        Ok(())
    }
}

impl Draw for Selection {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        let (_screen_width, screen_height) = canvas.size()?;
        canvas.clear()?;

        let item_idx_lower = self.item_cursor;
        let max_upper = self.item_cursor + screen_height;
        let item_idx_upper = min(max_upper, self.items.len());

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

fn build_compare_function(criterion: Vec<RankCriteria>) -> CompareFunction<MatchedItem> {
    use std::cmp::Ordering as CmpOrd;
    Box::new(move |a: &MatchedItem, b: &MatchedItem| {
        for &criteria in criterion.iter() {
            match criteria {
                RankCriteria::Begin => {
                    if a.rank.begin == b.rank.begin {
                        continue;
                    } else {
                        return a.rank.begin.cmp(&b.rank.begin);
                    }
                }
                RankCriteria::NegBegin => {
                    if a.rank.begin == b.rank.begin {
                        continue;
                    } else {
                        return b.rank.begin.cmp(&a.rank.begin);
                    }
                }
                RankCriteria::End => {
                    if a.rank.end == b.rank.end {
                        continue;
                    } else {
                        return a.rank.end.cmp(&b.rank.end);
                    }
                }
                RankCriteria::NegEnd => {
                    if a.rank.end == b.rank.end {
                        continue;
                    } else {
                        return b.rank.end.cmp(&a.rank.end);
                    }
                }
                RankCriteria::Index => {
                    if a.rank.index == b.rank.index {
                        continue;
                    } else {
                        return a.rank.index.cmp(&b.rank.index);
                    }
                }
                RankCriteria::NegIndex => {
                    if a.rank.index == b.rank.index {
                        continue;
                    } else {
                        return b.rank.index.cmp(&a.rank.index);
                    }
                }
                RankCriteria::Score => {
                    if a.rank.score == b.rank.score {
                        continue;
                    } else {
                        return a.rank.score.cmp(&b.rank.score);
                    }
                }
                RankCriteria::NegScore => {
                    if a.rank.score == b.rank.score {
                        continue;
                    } else {
                        return b.rank.score.cmp(&a.rank.score);
                    }
                }
            }
        }
        CmpOrd::Equal
    })
}

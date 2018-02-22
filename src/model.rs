use std::sync::mpsc::Sender;
use event::{Event, EventReceiver};
use item::{MatchedItem, MatchedItemGroup, MatchedRange};
use std::cmp::{max, min};
use orderedvec::OrderedVec;
use std::sync::Arc;
use std::collections::HashMap;
use unicode_width::UnicodeWidthChar;
use curses::*;
use std::process::Command;
use std::error::Error;
use ansi::ANSIParser;
use std::default::Default;
use regex::{Captures, Regex};
use field::get_string_by_range;
use std::borrow::Cow;
use std::convert::From;
use clap::ArgMatches;
use std::time::{Duration, Instant};

pub type ClosureType = Box<Fn(&mut Window) + Send>;

const SPINNER_DURATION: u32 = 200;
const SPINNERS: [char; 8] = ['-', '\\', '|', '/', '-', '\\', '|', '/'];

lazy_static! {
    static ref RE_FILEDS: Regex = Regex::new(r"(\{[0-9.,q]*?})").unwrap();
    static ref REFRESH_DURATION: Duration = Duration::from_millis(200);
}

pub struct Model {
    rx_cmd: EventReceiver,
    items: OrderedVec<Arc<MatchedItem>>, // all items
    selected: HashMap<(usize, usize), Arc<MatchedItem>>,

    item_cursor: usize, // the index of matched item currently highlighted.
    line_cursor: usize, // line No.
    hscroll_offset: usize,
    height: u16,
    width: u16,

    pub tabstop: usize,

    reader_stopped: bool,
    matcher_stopped: bool,
    num_read: usize,
    num_processed: usize,
    matcher_mode: String,
    timer: Instant,

    preview_hidden: bool,

    // Options
    multi_selection: bool,
    reverse: bool,
    preview_cmd: Option<String>,
    delimiter: Regex,
    output_ending: &'static str,
    print_query: bool,
    print_cmd: bool,
    no_hscroll: bool,
}

impl Model {
    pub fn new(rx_cmd: EventReceiver) -> Self {
        Model {
            rx_cmd: rx_cmd,
            items: OrderedVec::new(),
            num_read: 0,
            num_processed: 0,
            selected: HashMap::new(),

            item_cursor: 0,
            line_cursor: 0,
            hscroll_offset: 0,
            height: 0,
            width: 0,

            tabstop: 8,

            reader_stopped: false,
            matcher_stopped: false,
            timer: Instant::now(),
            matcher_mode: "".to_string(),

            preview_hidden: true,

            multi_selection: false,
            reverse: false,
            preview_cmd: None,
            delimiter: Regex::new(r"[ \t\n]+").unwrap(),
            output_ending: "\n",
            print_query: false,
            print_cmd: false,
            no_hscroll: false,
        }
    }

    pub fn parse_options(&mut self, options: &ArgMatches) {
        if options.is_present("multi") {
            self.multi_selection = true;
        }

        if options.is_present("no-multi") {
            self.multi_selection = false;
        }

        if options.is_present("reverse") {
            self.reverse = true;
        }

        if let Some(preview_cmd) = options.values_of("preview").and_then(|vals| vals.last()) {
            self.preview_cmd = Some(preview_cmd.to_string());
        }

        if let Some(preview_window) = options
            .values_of("preview-window")
            .and_then(|vals| vals.last())
        {
            self.preview_hidden = preview_window.find("hidden").is_some();
        }

        if let Some(delimiter) = options.values_of("delimiter").and_then(|vals| vals.last()) {
            self.delimiter = Regex::new(delimiter).unwrap_or_else(|_| Regex::new(r"[ \t\n]+").unwrap());
        }

        if options.is_present("print0") {
            self.output_ending = "\0";
        }

        if options.is_present("print-query") {
            self.print_query = true;
        }

        if options.is_present("print-cmd") {
            self.print_cmd = true;
        }

        if options.is_present("no-hscroll") {
            self.no_hscroll = true;
        }

        if let Some(tabstop_str) = options.values_of("tabstop").and_then(|vals| vals.last()) {
            let tabstop = tabstop_str.parse::<usize>().unwrap_or(8);
            self.tabstop = max(1, tabstop);
        }
    }

    pub fn run(&mut self, mut curses: Curses) {
        // generate a new instance of curses for printing
        //
        let mut last_refresh = Instant::now();

        // main loop
        loop {
            // check for new item
            if let Ok((ev, arg)) = self.rx_cmd.recv() {
                debug!("model: got {:?}", ev);
                match ev {
                    Event::EvModelNewItem => {
                        let items: MatchedItemGroup = *arg.downcast().unwrap();
                        self.insert_new_items(items);
                    }

                    Event::EvModelRestart => {
                        // clean the model
                        self.clean_model();
                    }

                    Event::EvModelDrawQuery => {
                        //debug!("model:EvModelDrawQuery:query");
                        let print_query_func = *arg.downcast::<ClosureType>().unwrap();
                        self.draw_query(&mut curses.win_main, &print_query_func);
                        curses.refresh();
                    }
                    Event::EvModelDrawInfo => {
                        //debug!("model:EvModelDrawInfo:status");
                        self.draw_status(&mut curses.win_main);
                        curses.refresh();
                    }

                    Event::EvModelNotifyProcessed => {
                        //debug!("model:EvModelNotifyProcessed:items_and_status");
                        let num_processed = *arg.downcast::<usize>().unwrap();
                        self.num_processed = num_processed;

                        if !self.reader_stopped {
                            // if the reader is still running, the number of processed items equals
                            // to the number of read items
                            self.num_read = num_processed;

                            let now = Instant::now();
                            let diff = now.duration_since(last_refresh);

                            // update the screen
                            // num_processed % 4096 == 0
                            if num_processed.trailing_zeros() >= 12 && diff > *REFRESH_DURATION {
                                self.act_redraw_items_and_status(&mut curses);
                                last_refresh = now;
                            }
                        }
                    }

                    Event::EvModelNotifyMatcherMode => {
                        self.matcher_mode = *arg.downcast().unwrap();
                    }

                    Event::EvMatcherStopped => {
                        //debug!("model:EvMatcherStopped:items_and_status");
                        self.matcher_stopped = true;
                        self.act_redraw_items_and_status(&mut curses);
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
                        let (accept_key, query, cmd, tx_ack): (
                            Option<String>,
                            String,
                            String,
                            Sender<usize>,
                        ) = *arg.downcast().unwrap();

                        // output query
                        if self.print_query {
                            print!("{}{}", query, self.output_ending);
                        }

                        if self.print_cmd {
                            print!("{}{}", cmd, self.output_ending);
                        }

                        accept_key.map(|key| {
                            print!("{}{}", key, self.output_ending);
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
                        //debug!("model:redraw_items_and_status");
                        self.act_move_line_cursor(1);
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActDown => {
                        //debug!("model:redraw_items_and_status");
                        self.act_move_line_cursor(-1);
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActToggle => {
                        //debug!("model:redraw_items_and_status");
                        self.act_toggle();
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActToggleDown => {
                        //debug!("model:redraw_items_and_status");
                        self.act_toggle();
                        self.act_move_line_cursor(-1);
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActToggleUp => {
                        //debug!("model:redraw_items_and_status");
                        self.act_toggle();
                        self.act_move_line_cursor(1);
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActToggleAll => {
                        //debug!("model:redraw_items_and_status");
                        self.act_toggle_all();
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActSelectAll => {
                        //debug!("model:redraw_items_and_status");
                        self.act_select_all();
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActDeselectAll => {
                        //debug!("model:redraw_items_and_status");
                        self.act_deselect_all();
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActPageDown => {
                        //debug!("model:redraw_items_and_status");
                        let height = 1 - i32::from(self.height);
                        self.act_move_line_cursor(height);
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActPageUp => {
                        //debug!("model:redraw_items_and_status");
                        let height = i32::from(self.height) - 1;
                        self.act_move_line_cursor(height);
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActScrollLeft => {
                        //debug!("model:redraw_items_and_status");
                        self.act_scroll(*arg.downcast::<i32>().unwrap_or_else(|_| Box::new(-1)));
                        self.act_redraw_items_and_status(&mut curses);
                    }
                    Event::EvActScrollRight => {
                        //debug!("model:redraw_items_and_status");
                        self.act_scroll(*arg.downcast::<i32>().unwrap_or_else(|_| Box::new(1)));
                        self.act_redraw_items_and_status(&mut curses);
                    }

                    Event::EvActTogglePreview => {
                        self.act_toggle_preview(&mut curses);
                        // main loop will send EvActRedraw afterwards
                        // so no need to call redraw here (besides, print_query_func is unknown)
                    }

                    Event::EvActRedraw => {
                        //debug!("model:EvActRedraw:act_redraw");
                        let print_query_func = *arg.downcast::<ClosureType>().unwrap();
                        self.act_redarw(&mut curses, print_query_func);
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

    fn update_size(&mut self, curses: &mut Window) {
        // update the (height, width)
        let (h, w) = curses.get_maxyx();
        self.height = h - 2;
        self.width = w - 2;
    }

    fn insert_new_items(&mut self, items: MatchedItemGroup) {
        for item in items {
            self.items.push(Arc::new(item));
        }
    }

    fn draw_items(&mut self, curses: &mut Window) {
        // cursor should be placed on query, so store cursor before printing
        let (old_y, old_x) = curses.getyx();

        let (h, _) = curses.get_maxyx();
        let h = h as usize;

        // screen-line: y         <--->   item-line: (height - y - 1)
        //              h-1                          h-(h-1)-1 = 0
        //              0                            h-1
        // screen-line: (h-l-1)   <--->   item-line: l

        let lower = self.item_cursor;
        let max_upper = self.item_cursor + h - 2;
        let upper = min(max_upper, self.items.len());

        for i in lower..upper {
            let l = i - lower;
            curses.mv((if self.reverse { l + 2 } else { h - 3 - l }) as u16, 0);
            // print the cursor label
            let label = if l == self.line_cursor { ">" } else { " " };
            curses.cprint(label, COLOR_CURSOR, true);

            let item = Arc::clone(self.items.get(i).unwrap());
            self.draw_item(curses, &item, l == self.line_cursor);
            curses.attr_on(0);
        }

        // clear rest of lines
        // It is an optimization to avoid flickering by avoid erasing contents and then draw new
        // ones
        for i in upper..max_upper {
            let l = i - lower;
            curses.mv((if self.reverse { l + 2 } else { h - 3 - l }) as u16, 0);
            curses.clrtoeol();
        }

        // restore cursor
        curses.mv(old_y, old_x);
    }

    fn draw_status(&self, curses: &mut Window) {
        // cursor should be placed on query, so store cursor before printing
        let (y, x) = curses.getyx();

        curses.mv(if self.reverse { 1 } else { self.height }, 0);
        curses.clrtoeol();

        // display spinner
        if self.reader_stopped {
            self.print_char(curses, ' ', COLOR_NORMAL, false);
        } else {
            let time = self.timer.elapsed();
            let mills = (time.as_secs() * 1000) as u32 + time.subsec_nanos() / 1000 / 1000;
            let index = (mills / SPINNER_DURATION) % (SPINNERS.len() as u32);
            self.print_char(curses, SPINNERS[index as usize], COLOR_SPINNER, true);
        }

        // display matched/total number
        curses.cprint(
            format!(" {}/{}", self.items.len(), self.num_read).as_ref(),
            COLOR_INFO,
            false,
        );

        // display the matcher mode
        if !self.matcher_mode.is_empty() {
            curses.cprint(
                format!("/{}", &self.matcher_mode).as_ref(),
                COLOR_INFO,
                false,
            );
        }

        // display the percentage of the number of processed items
        if self.num_processed < self.num_read {
            curses.cprint(
                format!(" ({}%) ", self.num_processed * 100 / self.num_read).as_ref(),
                COLOR_INFO,
                false,
            )
        }

        // selected number
        if self.multi_selection && !self.selected.is_empty() {
            curses.cprint(
                format!(" [{}]", self.selected.len()).as_ref(),
                COLOR_INFO,
                true,
            );
        }

        // item cursor
        let line_num_str = format!(" {} ", self.item_cursor + self.line_cursor);
        curses.mv(
            if self.reverse { 1 } else { self.height },
            self.width - (line_num_str.len() as u16),
        );
        curses.cprint(&line_num_str, COLOR_INFO, true);

        // restore cursor
        curses.mv(y, x);
    }

    fn draw_query(&self, curses: &mut Window, print_query_func: &ClosureType) {
        let (h, w) = curses.get_maxyx();
        let (h, _) = (h as usize, w as usize);

        //debug!("model:draw_query");

        // print query
        curses.mv((if self.reverse { 0 } else { h - 1 }) as u16, 0);
        curses.clrtoeol();
        print_query_func(curses);
    }

    fn draw_item(&self, curses: &mut Window, matched_item: &MatchedItem, is_current: bool) {
        let index = matched_item.item.get_full_index();

        if self.selected.contains_key(&index) {
            curses.cprint(">", COLOR_SELECTED, true);
        } else {
            curses.cprint(
                " ",
                if is_current {
                    COLOR_CURRENT
                } else {
                    COLOR_NORMAL
                },
                false,
            );
        }

        let (y, x) = curses.getyx();

        debug!("model:draw_item: {:?}", matched_item);
        let (match_start, match_end) = match matched_item.matched_range {
            Some(MatchedRange::Chars(ref matched_indics)) => {
                if !matched_indics.is_empty() {
                    (
                        matched_indics[0],
                        matched_indics[matched_indics.len() - 1] + 1,
                    )
                } else {
                    (0, 0)
                }
            }
            Some(MatchedRange::Range(match_start, match_end)) => (match_start, match_end),
            None => (0, 0),
        };

        let item = &matched_item.item;
        let text: Vec<_> = item.get_text().chars().collect();
        let (shift, full_width) = reshape_string(
            &text,
            self.width as usize,
            match_start,
            match_end,
            self.tabstop,
        );

        debug!(
            "model:draw_item: shfit: {:?}, width:{:?}, full_width: {:?}",
            shift, self.width, full_width
        );
        let mut printer = LinePrinter::builder()
            .tabstop(self.tabstop)
            .container_width(self.width as usize)
            .shift(if self.no_hscroll { 0 } else { shift })
            .text_width(full_width)
            .hscroll_offset(self.hscroll_offset)
            .build();

        // print out the original content
        curses.mv(y, x);
        printer.reset();
        if is_current {
            curses.attr_on(COLOR_CURRENT);
        }

        let mut ansi_states = item.get_ansi_states().iter().peekable();
        for (ch_idx, &ch) in text.iter().enumerate() {
            // print ansi color codes.
            while let Some(&&(ansi_idx, attr)) = ansi_states.peek() {
                if ch_idx == ansi_idx {
                    if is_current && ansi_contains_reset(attr) {
                        curses.attr_on(COLOR_CURRENT);
                    } else {
                        curses.attr_on(attr);
                    }
                    let _ = ansi_states.next();
                } else if ch_idx > ansi_idx {
                    let _ = ansi_states.next();
                } else {
                    break;
                }
            }
            printer.print_char(curses, ch, COLOR_NORMAL, false, false);
        }
        curses.attr_on(0);
        curses.clrtoeol();

        // print the highlighted content
        curses.mv(y, x);
        printer.reset();
        match matched_item.matched_range {
            Some(MatchedRange::Chars(ref matched_indics)) => {
                let mut matched_indics_iter = matched_indics.iter().peekable();

                for (ch_idx, &ch) in text.iter().enumerate() {
                    match matched_indics_iter.peek() {
                        Some(&&match_idx) if ch_idx == match_idx => {
                            printer.print_char(curses, ch, COLOR_MATCHED, is_current, false);
                            let _ = matched_indics_iter.next();
                        }
                        Some(_) | None => {
                            printer.print_char(curses, ch, COLOR_NORMAL, false, true);
                        }
                    }
                }
                curses.attr_on(0);
            }

            Some(MatchedRange::Range(start, end)) => {
                for (idx, &ch) in text.iter().enumerate() {
                    printer.print_char(
                        curses,
                        ch,
                        COLOR_MATCHED,
                        is_current,
                        !(idx >= start && idx < end),
                    );
                }
                curses.attr_on(0);
            }

            _ => {}
        }
    }

    fn draw_preview(&mut self, curses: &mut Window) {
        if self.preview_hidden {
            return;
        }

        curses.draw_border();
        if self.preview_cmd.is_none() {
            return;
        }

        curses.attr_on(0);

        // cursor should be placed on query, so store cursor before printing
        let (lines, cols) = curses.get_maxyx();

        let current_idx = self.item_cursor + self.line_cursor;
        if current_idx >= self.items.len() {
            curses.clrtoend();
            return;
        }

        let item = Arc::clone(self.items.get(current_idx).unwrap());
        let highlighted_content = item.item.get_text();

        debug!(
            "model:draw_preview: highlighted_content: '{:?}'",
            highlighted_content
        );
        let cmd = self.inject_preview_command(highlighted_content);
        debug!("model:draw_preview: cmd: '{:?}'", cmd);

        let output = match get_command_output(&cmd, lines, cols) {
            Ok(output) => output,
            Err(e) => format!("{}\n{}", cmd, e.description()),
        };
        debug!("model:draw_preview: output: '{:?}'", output);

        let mut ansi_parser: ANSIParser = Default::default();
        let (strip_string, ansi_states) = ansi_parser.parse_ansi(&output);

        debug!("model:draw_preview: output = {:?}", &output);
        debug!(
            "model:draw_preview: strip_string: {:?}\nansi_states: {:?}",
            strip_string, ansi_states
        );

        let mut ansi_states = ansi_states.iter().peekable();

        curses.mv(0, 0);
        for (ch_idx, ch) in strip_string.chars().enumerate() {
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
            curses.addch(ch);
        }
        curses.attr_on(0);

        curses.clrtoend();
    }

    fn inject_preview_command<'a>(&'a self, text: &str) -> Cow<'a, str> {
        let cmd = self.preview_cmd.as_ref().unwrap();
        debug!("replace: {:?}, text: {:?}", cmd, text);
        RE_FILEDS.replace_all(cmd, |caps: &Captures| {
            assert!(caps[1].len() >= 2);
            let range = &caps[1][1..caps[1].len() - 1];
            if range == "" {
                format!("'{}'", text)
            } else {
                format!(
                    "'{}'",
                    get_string_by_range(&self.delimiter, text, range).unwrap_or("")
                )
            }
        })
    }

    fn print_char(&self, curses: &mut Window, ch: char, color: u16, is_bold: bool) {
        if ch != '\t' {
            curses.caddch(ch, color, is_bold);
        } else {
            // handle tabstop
            let (_, x) = curses.getyx();
            let rest = self.tabstop - (x as usize - 2) % self.tabstop;
            for _ in 0..rest {
                curses.caddch(' ', color, is_bold);
            }
        }
    }

    //--------------------------------------------------------------------------
    // Actions

    pub fn act_move_line_cursor(&mut self, diff: i32) {
        let diff = if self.reverse { -diff } else { diff };
        let mut line_cursor = self.line_cursor as i32;
        let mut item_cursor = self.item_cursor as i32;
        let item_len = self.items.len() as i32;

        let height = i32::from(self.height);

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

        let current_item = self.items.get(self.item_cursor + self.line_cursor).unwrap();
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
            let current_item = self.items.get(self.item_cursor + self.line_cursor).unwrap();
            let index = current_item.item.get_full_index();
            self.selected.insert(index, Arc::clone(current_item));
        }

        let mut output: Vec<_> = self.selected.iter_mut().collect::<Vec<_>>();
        output.sort_by_key(|k| k.0);
        for (_, item) in output {
            print!("{}{}", item.item.get_output_text(), self.output_ending);
        }
    }

    pub fn act_toggle_preview(&mut self, curses: &mut Curses) {
        self.preview_hidden = !self.preview_hidden;
        curses.toggle_preview_window();
    }

    pub fn act_scroll(&mut self, offset: i32) {
        let mut hscroll_offset = self.hscroll_offset as i32;
        hscroll_offset += offset;
        hscroll_offset = max(0, hscroll_offset);
        self.hscroll_offset = hscroll_offset as usize;
    }

    pub fn act_redarw(&mut self, curses: &mut Curses, print_query_func: ClosureType) {
        curses.resize();
        self.update_size(&mut curses.win_main);
        self.draw_preview(&mut curses.win_preview);
        self.draw_items(&mut curses.win_main);
        self.draw_status(&mut curses.win_main);
        self.draw_query(&mut curses.win_main, &print_query_func);
        curses.refresh();
    }

    fn act_redraw_items_and_status(&mut self, curses: &mut Curses) {
        curses.win_main.hide_cursor();
        self.draw_preview(&mut curses.win_preview);
        self.draw_items(&mut curses.win_main);
        self.draw_status(&mut curses.win_main);
        curses.win_main.show_cursor();
        curses.refresh();
    }
}

fn get_command_output(cmd: &str, lines: u16, cols: u16) -> Result<String, Box<Error>> {
    let output = Command::new("sh")
        .env("LINES", lines.to_string())
        .env("COLUMNS", cols.to_string())
        .arg("-c")
        .arg(cmd)
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let error: Box<Error> = From::from(String::from_utf8_lossy(&output.stderr).to_string());
        Err(error)
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
    current: i32,

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
            current: -1,

            tabstop: 8,
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
        self.reset();
        self
    }

    pub fn reset(&mut self) {
        self.current = 0;
        self.start = self.shift + self.hscroll_offset;
        self.end = self.start + self.container_width;
    }

    fn caddch(&mut self, curses: &mut Window, ch: char, color: u16, is_bold: bool, skip: bool) {
        let w = ch.width().unwrap_or(2);

        if skip {
            curses.move_cursor_right(w as u16);
        } else {
            curses.caddch(ch, color, is_bold);
        }
    }

    fn print_char_raw(&mut self, curses: &mut Window, ch: char, color: u16, is_bold: bool, skip: bool) {
        // hide the content that outside the screen, and show the hint(i.e. `..`) for overflow
        // the hidden chracter

        let w = ch.width().unwrap_or(2);

        assert!(self.current >= 0);
        let current = self.current as usize;

        if current < self.start || current >= self.end {
            // pass if it is hidden
        } else if current < self.start + 2 && (self.shift > 0 || self.hscroll_offset > 0) {
            // print left ".."
            for _ in 0..min(w, current - self.start + 1) {
                self.caddch(curses, '.', color, is_bold, skip);
            }
        } else if self.end - current <= 2 && (self.text_width > self.end) {
            // print right ".."
            for _ in 0..min(w, self.end - current) {
                self.caddch(curses, '.', color, is_bold, skip);
            }
        } else {
            self.caddch(curses, ch, color, is_bold, skip);
        }

        self.current += w as i32;
    }

    pub fn print_char(&mut self, curses: &mut Window, ch: char, color: u16, is_bold: bool, skip: bool) {
        if ch != '\t' {
            self.print_char_raw(curses, ch, color, is_bold, skip);
        } else {
            // handle tabstop
            let rest = if self.current < 0 {
                self.tabstop
            } else {
                self.tabstop - (self.current as usize) % self.tabstop
            };
            for _ in 0..rest {
                self.print_char_raw(curses, ' ', color, is_bold, skip);
            }
        }
    }
}

//==============================================================================
// helper functions

// return an array, arr[i] store the display width till char[i]
fn accumulate_text_width(text: &[char], tabstop: usize) -> Vec<usize> {
    let mut ret = Vec::new();
    let mut w = 0;
    for &ch in text.iter() {
        w += if ch == '\t' {
            tabstop - (w % tabstop)
        } else {
            ch.width().unwrap_or(2)
        };
        ret.push(w);
    }
    ret
}

// "smartly" calculate the "start" position of the string in order to show the matched contents
// for example, if the match appear in the end of a long string, we need to show the right part.
// `xxxxxxxxxxxxxxxxxxxxxxxxxxMMxxxxxMxxxxx`
//                shift ->|               |
//
// return (left_shift, full_print_width)
fn reshape_string(
    text: &[char],
    container_width: usize,
    match_start: usize,
    match_end: usize,
    tabstop: usize,
) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }

    let acc_width = accumulate_text_width(text, tabstop);
    let full_width = acc_width[acc_width.len() - 1];
    if full_width <= container_width {
        return (0, full_width);
    }

    // w1, w2, w3 = len_before_matched, len_matched, len_after_matched
    let w1 = if match_start == 0 {
        0
    } else {
        acc_width[match_start - 1]
    };
    let w2 = if match_end >= text.len() {
        full_width - w1
    } else {
        acc_width[match_end] - w1
    };
    let w3 = acc_width[acc_width.len() - 1] - w1 - w2;

    if (w1 > w3 && w2 + w3 <= container_width) || (w3 <= 2) {
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
    use super::{accumulate_text_width, reshape_string};

    fn to_chars(s: &str) -> Vec<char> {
        s.to_string().chars().collect()
    }

    #[test]
    fn test_accumulate_text_width() {
        assert_eq!(
            accumulate_text_width(&to_chars(&"abcdefg"), 8),
            vec![1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(
            accumulate_text_width(&to_chars(&"ab中de国g"), 8),
            vec![1, 2, 4, 5, 6, 8, 9]
        );
        assert_eq!(
            accumulate_text_width(&to_chars(&"ab\tdefg"), 8),
            vec![1, 2, 8, 9, 10, 11, 12]
        );
        assert_eq!(
            accumulate_text_width(&to_chars(&"ab中\te国g"), 8),
            vec![1, 2, 4, 8, 9, 11, 12]
        );
    }

    #[test]
    fn test_reshape_string() {
        // no match, left fixed to 0
        assert_eq!(reshape_string(&to_chars(&"abc"), 10, 0, 0, 8), (0, 3));
        assert_eq!(reshape_string(&to_chars(&"a\tbc"), 8, 0, 0, 8), (0, 10));
        assert_eq!(reshape_string(&to_chars(&"a\tb\tc"), 10, 0, 0, 8), (0, 17));
        assert_eq!(
            reshape_string(&to_chars(&"a\t中b\tc"), 8, 0, 0, 8),
            (0, 17)
        );
        assert_eq!(
            reshape_string(&to_chars(&"a\t中b\tc012345"), 8, 0, 0, 8),
            (0, 23)
        );
    }
}

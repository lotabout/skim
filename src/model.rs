use ansi::AnsiString;
use curses::*;
use event::{Event, EventReceiver};
use field::get_string_by_range;
use item::{Item, MatchedItem, MatchedItemGroup, MatchedRange};
use options::SkimOptions;
use orderedvec::OrderedVec;
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::cmp::{max, min};
use std::collections::HashMap;
use std::convert::From;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthChar;
use util::escape_single_quote;
use previewer::PreviewInput;

// write query & returns (y,x) after query
pub type QueryPrintClosure = Box<Fn(&mut Window) -> (u16, u16) + Send>;

const SPINNER_DURATION: u32 = 200;
const SPINNERS: [char; 8] = ['-', '\\', '|', '/', '-', '\\', '|', '/'];
const DELIMITER_STR: &'static str = r"[\t\n ]+";

lazy_static! {
    static ref RE_FIELDS: Regex = Regex::new(r"\\?(\{-?[0-9.,q]*?})").unwrap();
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
    query_end_x: u16,

    reserved_height: u16, // sum of lines needed for: query, status & headers

    pub tabstop: usize,

    reader_stopped: bool,
    matcher_stopped: bool,
    num_read: usize,
    num_processed: usize,
    matcher_mode: String,
    timer: Instant,

    preview_hidden: bool,
    headers: Vec<AnsiString>,

    otx_preview: Option<Sender<(Event, PreviewInput)>>,

    // Options
    multi_selection: bool,
    reverse: bool,
    preview_cmd: Option<String>,
    delimiter: Regex,
    output_ending: &'static str,
    print_query: bool,
    print_cmd: bool,
    no_hscroll: bool,
    inline_info: bool
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
            reserved_height: 2, // = status + query (lines)
            query_end_x: 0,
            tabstop: 8,

            reader_stopped: false,
            matcher_stopped: false,
            timer: Instant::now(),
            matcher_mode: "".to_string(),

            preview_hidden: true,
            headers: Vec::new(),

            otx_preview: None,

            multi_selection: false,
            reverse: false,
            preview_cmd: None,
            delimiter: Regex::new(DELIMITER_STR).unwrap(),
            output_ending: "\n",
            print_query: false,
            print_cmd: false,
            no_hscroll: false,
            inline_info: false
        }
    }

    pub fn parse_options(&mut self, options: &SkimOptions) {
        if options.multi {
            self.multi_selection = true;
        }

        if options.reverse {
            self.reverse = true;
        }

        if let Some(preview_cmd) = options.preview {
            self.preview_cmd = Some(preview_cmd.to_string());
        }

        if let Some(preview_window) = options.preview_window {
            self.preview_hidden = preview_window.find("hidden").is_some();
        }

        if let Some(delimiter) = options.delimiter {
            self.delimiter = Regex::new(delimiter).unwrap_or_else(|_| Regex::new(DELIMITER_STR).unwrap());
        }

        if options.print0 {
            self.output_ending = "\0";
        }

        if options.print_query {
            self.print_query = true;
        }

        if options.print_cmd {
            self.print_cmd = true;
        }

        if options.no_hscroll {
            self.no_hscroll = true;
        }

        if let Some(tabstop_str) = options.tabstop {
            let tabstop = tabstop_str.parse::<usize>().unwrap_or(8);
            self.tabstop = max(1, tabstop);
        }

        if options.inline_info {
            self.reserved_height = 1;
            self.inline_info = true;
        }

        match options.header{
            None => {},
            Some("") => {},
            Some(header) => {
                self.reserved_height += 1;
                self.headers.push(AnsiString::from_str(header));
            }
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
                        let items: MatchedItemGroup =
                            *arg.downcast().expect("model:EvModelNewItem: failed to get argument");
                        self.insert_new_items(items);
                    }

                    Event::EvModelRestart => {
                        // clean the model
                        self.clean_model();
                    }

                    Event::EvModelDrawQuery => {
                        //debug!("model:EvModelDrawQuery:query");
                        let print_query_func = *arg.downcast::<QueryPrintClosure>()
                            .expect("model:EvModelDrawQuery: failed to get argument");
                        self.draw_query(&mut curses.win_main, &print_query_func);
                        curses.refresh();
                    }
                    Event::EvModelDrawInfo => {
                        //debug!("model:EvModelDrawInfo:status");
                        self.draw_status(&mut curses.win_main);
                        curses.refresh();
                    }
                    Event::EvModelNewPreview => {
                        //debug!("model:EvModelNewPreview:handle_preview_output");
                        let preview_output = *arg.downcast::<AnsiString>()
                            .expect("model:EvModelNewPreview: failed to get argument");
                        self.handle_preview_output(&mut curses.win_preview, preview_output);
                    }

                    Event::EvModelNotifyProcessed => {
                        //debug!("model:EvModelNotifyProcessed:items_and_status");
                        let num_processed = *arg.downcast::<usize>()
                            .expect("model:EvModelNotifyProcessed: failed to get argument");
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
                        self.matcher_mode = *arg.downcast()
                            .expect("model:EvModelNotifyMatcherMode: failed to get argument");
                    }

                    Event::EvMatcherStopped => {
                        //debug!("model:EvMatcherStopped:items_and_status");
                        self.matcher_stopped = true;
                        self.act_redraw_items_and_status(&mut curses);
                    }

                    Event::EvReaderStopped => {
                        // if reader stopped, the num_read is freezed.
                        self.reader_stopped = true;
                        self.num_read = *arg.downcast().expect("model:EvReaderStopped: failed to get argument");
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
                        let tx_ack: Sender<Vec<Arc<Item>>> =
                            *arg.downcast().expect("model:EvActAccept: failed to get argument");

                        // do the final dirty work
                        self.act_output();

                        let mut selected: Vec<Arc<Item>> =
                            self.selected.values().map(|item| item.item.clone()).collect();

                        selected.sort_by_key(|item| item.get_full_index());

                        // return the selected items
                        let _ = tx_ack.send(selected);
                        break;
                    }
                    Event::EvActAbort => {
                        if let Some(tx_preview) = &self.otx_preview{
                            tx_preview.send((Event::EvActAbort,
                                             PreviewInput{cmd: "".into(), lines: 0, columns:0}))
                                .expect("Failed to send to tx_preview");
                        }
                        let tx_ack: Sender<bool> = *arg.downcast().expect("model:EvActAbort: failed to get argument");
                        curses.close();
                        let _ = tx_ack.send(true);
                        break;
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
                        let print_query_func = *arg.downcast::<QueryPrintClosure>()
                            .expect("model:EvActRedraw: failed to get argument");
                        self.act_redraw(&mut curses, print_query_func);
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
        self.height = h - self.reserved_height;
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
        let max_upper = self.item_cursor + h - (self.reserved_height as usize);
        let upper = min(max_upper, self.items.len());

        for i in lower..upper {
            let l = i - lower;
            curses.mv(self.get_item_height(l, h), 0);
            // print the cursor label
            let label = if l == self.line_cursor { ">" } else { " " };
            curses.cprint(label, COLOR_CURSOR, true);

            let item = Arc::clone(
                self.items
                    .get(i)
                    .expect(format!("model:draw_items: failed to get item at {}", i).as_str()),
            );
            self.draw_item(curses, &item, l == self.line_cursor);
            curses.attr_on(0);
        }

        // clear rest of lines
        // It is an optimization to avoid flickering by avoid erasing contents and then draw new
        // ones
        for i in upper..max_upper {
            let l = i - lower;
            curses.mv(self.get_item_height(l, h) as u16, 0);
            curses.clrtoeol();
        }

        // restore cursor
        curses.mv(old_y, old_x);
    }

    fn get_item_height(&self, l: usize, h: usize) -> u16 {
        let res = if self.reverse {
            l + (self.reserved_height as usize)
        } else {
            h - (self.reserved_height as usize) - 1 - l
        };
        res as u16
    }

    fn get_status_position(&self, cursor_y: u16) -> (u16, u16) {
        match (self.inline_info, self.reverse){
            (false, true) => (1, 0),
            (false, false) => ({ self.height + self.reserved_height - 2 }, 0),
            (true, _) => ((cursor_y, self.query_end_x))
        }
    }

    fn draw_status(&self, curses: &mut Window) {
        // cursor should be placed on query, so store cursor before printing
        let (y, x) = curses.getyx();

        let (status_y, status_x) = self.get_status_position(y);

        curses.mv(status_y, status_x);
        curses.clrtoeol();

        if self.inline_info{
            curses.cprint("  <", COLOR_PROMPT, false);
        };

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
            curses.cprint(format!("/{}", &self.matcher_mode).as_ref(), COLOR_INFO, false);
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
            curses.cprint(format!(" [{}]", self.selected.len()).as_ref(), COLOR_INFO, true);
        }

        // item cursor
        let line_num_str = format!(" {} ", self.item_cursor + self.line_cursor);
        curses.mv(
            status_y,
            self.width - (line_num_str.len() as u16),
        );
        curses.cprint(&line_num_str, COLOR_INFO, true);

        // restore cursor
        curses.mv(y, x);
    }

    fn get_header_height(&self, query_y: u16, maxy:u16) -> Option<u16> {
        let (status_height, _) = self.get_status_position(query_y);
        let res = if self.reverse {status_height + 1} else {status_height - 1};

        if self.reserved_height +1 < maxy && maxy > 3 {
            Some(res)
         } else {
            None
        }
    }

    fn draw_headers(&self, curses: &mut Window) {
        // cursor should be placed on query, so store cursor before printing
        let (y, x) = curses.getyx();
        let (maxy, _) = curses.get_maxyx();
        let (has_headers, yh) = (self.headers.len() > 0, self.get_header_height( y, maxy));
        if ! has_headers || yh.is_none() {
            return;
        }
        let yh = yh.unwrap();
        let direction = if self.reverse {1} else {-1};

        let mut printer = LinePrinter::builder()
            .container_width(self.width as usize)
            .shift(0)
            .hscroll_offset(self.hscroll_offset)
            .build();

        for (i, header) in self.headers.iter().enumerate() {
            let nyh = ((yh as i64)+(direction*(i as i64))) as u16;
            curses.mv(nyh, 0);
            curses.clrtoeol();
            curses.mv(nyh, 2);
            for (ch, attrs) in header.iter(){
                for (_, attr) in attrs {
                    curses.attr_on(*attr);
                }
                printer.print_char(curses, ch, COLOR_NORMAL, false, false);
            }

        }
        // restore cursor
        curses.mv(y, x);
    }

    fn draw_query(&mut self, curses: &mut Window, print_query_func: &QueryPrintClosure) {
        let (h, w) = curses.get_maxyx();
        let (h, _) = (h as usize, w as usize);

        //debug!("model:draw_query");

        // print query
        curses.mv((if self.reverse { 0 } else { h - 1 }) as u16, 0);
        if ! self.inline_info {
            curses.clrtoeol();
        }
        let (_, x) = print_query_func(curses);
        self.query_end_x = x;

    }

    fn draw_item(&self, curses: &mut Window, matched_item: &MatchedItem, is_current: bool) {
        let index = matched_item.item.get_full_index();

        if self.selected.contains_key(&index) {
            curses.cprint(">", COLOR_SELECTED, true);
        } else {
            curses.cprint(" ", if is_current { COLOR_CURRENT } else { COLOR_NORMAL }, false);
        }

        let (y, x) = curses.getyx();

        debug!("model:draw_item: {:?}", matched_item);
        let (match_start, match_end) = match matched_item.matched_range {
            Some(MatchedRange::Chars(ref matched_indics)) => {
                if !matched_indics.is_empty() {
                    (matched_indics[0], matched_indics[matched_indics.len() - 1] + 1)
                } else {
                    (0, 0)
                }
            }
            Some(MatchedRange::Range(match_start, match_end)) => (match_start, match_end),
            None => (0, 0),
        };

        let item = &matched_item.item;
        let text: Vec<_> = item.get_text().chars().collect();
        let (shift, full_width) = reshape_string(&text, self.width as usize, match_start, match_end, self.tabstop);

        debug!(
            "model:draw_item: shift: {:?}, width:{:?}, full_width: {:?}",
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

        if item.get_text_struct().is_some() && item.get_text_struct().as_ref().unwrap().has_attrs() {
            for (ch, attrs) in item.get_text_struct().as_ref().unwrap().iter(){
                for (_, attr) in attrs {
                    if is_current && ansi_contains_reset(*attr) {
                        curses.attr_on(COLOR_CURRENT);
                    } else {
                        curses.attr_on(*attr);
                    }
                }
                printer.print_char(curses, ch, COLOR_NORMAL, false, false);
            }
        } else {
            for ch in item.get_orig_text().chars(){
                printer.print_char(curses, ch, COLOR_NORMAL, false, false);
            }
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
                    printer.print_char(curses, ch, COLOR_MATCHED, is_current, !(idx >= start && idx < end));
                }
                curses.attr_on(0);
            }

            _ => {}
        }
    }

    pub fn set_previewer(&mut self, tx_preview: Sender<(Event, PreviewInput)>){
        self.otx_preview = Some(tx_preview);
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

        let item = Arc::clone(
            self.items
                .get(current_idx)
                .expect(format!("model:draw_items: failed to get item at {}", current_idx).as_str()),
        );
        let highlighted_content = item.item.get_orig_text();

        debug!("model:draw_preview: highlighted_content: '{:?}'", highlighted_content);
        let cmd = self.inject_preview_command(&highlighted_content);
        debug!("model:draw_preview: cmd: '{:?}'", cmd);

        if let Some(tx_preview) = &self.otx_preview {
            tx_preview.send((Event::EvModelNewPreview , PreviewInput{
                cmd: cmd.to_string().clone(),
                lines: lines,
                columns: cols
            })).expect("failed to send to previewer");
        }
    }

    fn handle_preview_output(&mut self, curses: &mut Window, aoutput: AnsiString){

        debug!("model:draw_preview: output = {:?}", &aoutput);

        curses.mv(0, 0);
        aoutput.print(curses);
        curses.attr_on(0);

        curses.clrtoend();
    }

    fn inject_preview_command<'a>(&'a self, text: &str) -> Cow<'a, str> {
        let cmd = self.preview_cmd
            .as_ref()
            .expect("model:inject_preview_command: invalid preview command");
        debug!("replace: {:?}, text: {:?}", cmd, text);
        RE_FIELDS.replace_all(cmd, |caps: &Captures| {
            // \{...
            if &caps[0][0..1] == "\\" {
                return caps[0].to_string();
            }

            // {1..} and other variant
            assert!(caps[1].len() >= 2);
            let range = &caps[1][1..caps[1].len() - 1];
            let replacement = if range == "" {
                text
            } else {
                get_string_by_range(&self.delimiter, text, range).unwrap_or("")
            };

            format!("'{}'", escape_single_quote(replacement))
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

        let cursor = self.item_cursor + self.line_cursor;
        let current_item = self.items
            .get(cursor)
            .expect(format!("model:act_toggle: failed to get item {}", cursor).as_str());
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
            let current_item = self.items
                .get(cursor)
                .expect(format!("model:act_output: failed to get item {}", cursor).as_str());
            let index = current_item.item.get_full_index();
            self.selected.insert(index, Arc::clone(current_item));
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

    pub fn act_redraw(&mut self, curses: &mut Curses, print_query_func: QueryPrintClosure) {
        curses.resize();
        self.update_size(&mut curses.win_main);
        self.draw_preview(&mut curses.win_preview);
        self.draw_items(&mut curses.win_main);
        self.draw_query(&mut curses.win_main, &print_query_func);
        self.draw_status(&mut curses.win_main);
        self.draw_headers(&mut curses.win_main);
        curses.refresh();
    }

    fn act_redraw_items_and_status(&mut self, curses: &mut Curses) {
        curses.win_main.hide_cursor();
        self.update_size(&mut curses.win_main);
        self.draw_preview(&mut curses.win_preview);
        self.draw_items(&mut curses.win_main);
        self.draw_status(&mut curses.win_main);
        curses.win_main.show_cursor();
        curses.refresh();
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
        assert_eq!(reshape_string(&to_chars(&"a\t中b\tc"), 8, 0, 0, 8), (0, 17));
        assert_eq!(reshape_string(&to_chars(&"a\t中b\tc012345"), 8, 0, 0, 8), (0, 23));
    }
}

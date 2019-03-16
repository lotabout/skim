use crate::ansi::AnsiString;
use crate::event::{Event, EventHandler, EventReceiver, EventSender, UpdateScreen};
use crate::field::get_string_by_range;
use crate::matcher::{Matcher, MatcherControl, MatcherMode};
use crate::options::SkimOptions;
use crate::previewer::PreviewInput;
use crate::query::Query;
use crate::reader::{Reader, ReaderControl};
use crate::selection::Selection;
use crate::spinlock::SpinLock;
use crate::theme::{ColorTheme, DEFAULT_THEME};
use crate::util::escape_single_quote;
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::cmp::{max, min};
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tuikit::prelude::*;
use unicode_width::UnicodeWidthChar;
use std::env;
use crate::output::SkimOutput;
use std::mem;
use crate::item::{Item, ItemPool};

const SPINNER_DURATION: u32 = 200;
const SPINNERS: [char; 8] = ['-', '\\', '|', '/', '-', '\\', '|', '/'];
const DELIMITER_STR: &str = r"[\t\n ]+";

lazy_static! {
    static ref RE_FIELDS: Regex = Regex::new(r"\\?(\{-?[0-9.,q]*?})").unwrap();
    static ref REFRESH_DURATION: Duration = Duration::from_millis(200);
}

pub struct Model {
    reader: Reader,
    query: Query,
    selection: Selection,
    matcher: Matcher,
    term: Arc<Term>,

    item_pool: Arc<ItemPool>,

    rx: EventReceiver,
    tx: EventSender,

    matcher_mode: String,
    timer: Instant,
    reader_control: Option<ReaderControl>,
    matcher_control: Option<MatcherControl>,

    preview_hidden: bool,
    headers: Vec<AnsiString>,

    tx_preview: Option<Sender<(Event, PreviewInput)>>,

    // Options
    reverse: bool,
    preview_cmd: Option<String>,
    delimiter: Regex,
    output_ending: &'static str,
    print_query: bool,
    print_cmd: bool,
    no_hscroll: bool,
    inline_info: bool,
    theme: Arc<ColorTheme>,
}

impl Model {
    pub fn new(rx: EventReceiver, tx: EventSender, reader: Reader, term: Arc<Term>, options: &SkimOptions) -> Self {
        let default_command = match env::var("SKIM_DEFAULT_COMMAND").as_ref().map(String::as_ref) {
            Ok("") | Err(_) => "find .".to_owned(),
            Ok(val) => val.to_owned(),
        };

        let theme = Arc::new(ColorTheme::init_from_options(options));
        let query = Query::from_options(&options)
            .replace_base_cmd_if_not_set(&default_command)
            .theme(theme.clone())
            .build();

        let selection = Selection::with_options(options).theme(theme.clone());
        let matcher = Matcher::with_options(options);

        let mut ret = Model {
            reader,
            query,
            selection,
            matcher,
            term,
            item_pool: Arc::new(ItemPool::new()),

            rx,
            tx,
            timer: Instant::now(),
            reader_control: None,
            matcher_control: None,
            matcher_mode: "".to_string(),

            preview_hidden: true,
            headers: Vec::new(),

            tx_preview: None,

            reverse: false,
            preview_cmd: None,
            delimiter: Regex::new(DELIMITER_STR).unwrap(),
            output_ending: "\n",
            print_query: false,
            print_cmd: false,
            no_hscroll: false,
            inline_info: false,
            theme,
        };
        ret.parse_options(options);
        ret
    }

    fn parse_options(&mut self, options: &SkimOptions) {
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

        if options.reverse {
            self.reverse = true;
        }

        if options.print_cmd {
            self.print_cmd = true;
        }

        if options.inline_info {
            self.inline_info = true;
        }

        match options.header {
            None => {}
            Some("") => {}
            Some(header) => {
                self.headers.push(AnsiString::from_str(header));
            }
        }
    }

    pub fn start(&mut self) -> Option<SkimOutput> {
        let mut cmd = self.query.get_cmd();
        let mut query = self.query.get_query();

        self.reader_control = Some(self.reader.run(&cmd));

        let mut redraw = UpdateScreen::Redraw;

        while let Ok((ev, arg)) = self.rx.recv() {
            debug!("model: ev: {:?}, arg: {:?}", ev, arg);
            if self.query.accept_event(ev) {
                redraw = self.query.handle(ev, arg);
                let new_query = self.query.get_query();
                let new_cmd = self.query.get_cmd();

                // re-run reader & matcher if needed;
                if new_cmd != cmd {
                    debug!("cmd: {:?}, new: {:?}", cmd, new_cmd);
                    cmd = new_cmd;

                    // stop matcher
                    self.reader_control.take().map(ReaderControl::kill);
                    self.matcher_control.take().map(|ctrl: MatcherControl| ctrl.kill());
                    self.selection.clear();
                    self.item_pool.clear();

                    // restart reader
                    self.reader_control.replace(self.reader.run(&cmd));
                } else if query != new_query {
                    query = new_query;

                    // restart matcher
                    self.matcher_control.take().map(|ctrl| ctrl.kill());
                    self.item_pool.reset();
                    self.selection.clear();
                    self.tx.send((Event::EvMatcherRestart, Box::new(true)));
                }
            } else if self.selection.accept_event(ev) {
                redraw = self.selection.handle(ev, arg);
            } else {

                match ev {
                    Event::EvHeartBeat => {
                        let processed = self.reader_control.as_ref().map(|c| c.is_processed()).unwrap_or(true);
                        // run matcher if matcher had been stopped and reader had new items.
                        if !processed && self.matcher_control.is_none() {
                            let _ = self.tx.send((Event::EvMatcherRestart, Box::new(true)));
                        }
                    }

                    Event::EvMatcherRestart => {
                        // if there are new items, move them to item pool
                        let processed = self.reader_control.as_ref().map(|c| c.is_processed()).unwrap_or(true);
                        if !processed {
                            // take out new items and put them into items
                            let mut new_items = self.reader_control.as_ref().map(|c| c.take()).unwrap();
                            self.item_pool.append(&mut new_items);
                        };

                        let tx_clone = self.tx.clone();
                        self.matcher_control.replace(self.matcher.run(&query, self.item_pool.clone(), None, move |_| {
                            let _ = tx_clone.send((Event::EvMatcherDone, Box::new(true)));
                        }));
                    }

                    Event::EvMatcherDone => {
                        // save the processed items
                        self.matcher_control.take().map(|ctrl| {
                            let lock = ctrl.into_items();
                            let mut items = lock.lock();
                            let matched = mem::replace(&mut *items, Vec::new());
                            self.selection.add_items(matched);
                            redraw = UpdateScreen::Redraw;
                        });

                        if !self.reader_control.as_ref().map(|c|c.is_processed()).unwrap_or(true) {
                            let _ = self.tx.send((Event::EvMatcherRestart, Box::new(true)));
                        }
                    }

                    Event::EvActAccept => {
                        self.reader_control.take().map(|ctrl| ctrl.kill());
                        self.matcher_control.take().map(|ctrl| ctrl.kill());
                        return Some(SkimOutput {
                            accept_key: None,
                            query: self.query.get_query(),
                            cmd: self.query.get_cmd_query(),
                            selected_items: self.selection.get_selected_items(),
                        });
                    }

                    _ => {}
                }
            }

            if redraw == UpdateScreen::Redraw {
                self.term.draw(self);
                self.term.present();
            }
            debug!("event done");
        }

        None
    }

//    pub fn run(&mut self, mut curses: Curses) {
//        // generate a new instance of curses for printing
//        //
//        let mut last_refresh = Instant::now();
//
//        // main loop
//        loop {
//            // check for new item
//            if let Ok((ev, arg)) = self.rx_cmd.recv() {
//                debug!("model: got {:?}", ev);
//                match ev {
//                    Event::EvModelDrawQuery => {
//                        //debug!("model:EvModelDrawQuery:query");
//                        let print_query_func = *arg
//                            .downcast::<QueryPrintClosure>()
//                            .expect("model:EvModelDrawQuery: failed to get argument");
//                        self.draw_query(&mut curses.win_main, &print_query_func);
//                        curses.refresh();
//                    }
//                    Event::EvModelDrawInfo => {
//                        //debug!("model:EvModelDrawInfo:status");
//                        self.draw_status(&mut curses.win_main);
//                        curses.refresh();
//                    }
//                    Event::EvModelNewPreview => {
//                        //debug!("model:EvModelNewPreview:handle_preview_output");
//                        let preview_output = *arg
//                            .downcast::<AnsiString>()
//                            .expect("model:EvModelNewPreview: failed to get argument");
//                        self.handle_preview_output(&mut curses.win_preview, preview_output);
//                    }
//
//                    Event::EvModelNotifyProcessed => {
//                        //debug!("model:EvModelNotifyProcessed:items_and_status");
//                        let num_processed = *arg
//                            .downcast::<usize>()
//                            .expect("model:EvModelNotifyProcessed: failed to get argument");
//                        self.num_processed = num_processed;
//
//                        if !self.reader_stopped {
//                            // if the reader is still running, the number of processed items equals
//                            // to the number of read items
//                            self.num_read = num_processed;
//
//                            let now = Instant::now();
//                            let diff = now.duration_since(last_refresh);
//
//                            // update the screen
//                            // num_processed % 4096 == 0
//                            if num_processed.trailing_zeros() >= 12 && diff > *REFRESH_DURATION {
//                                self.act_redraw_items_and_status(&mut curses);
//                                last_refresh = now;
//                            }
//                        }
//                    }
//
//                    Event::EvModelNotifyMatcherMode => {
//                        self.matcher_mode = *arg
//                            .downcast()
//                            .expect("model:EvModelNotifyMatcherMode: failed to get argument");
//                    }
//
//                    Event::EvMatcherStopped => {
//                        //debug!("model:EvMatcherStopped:items_and_status");
//                        self.matcher_stopped = true;
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//
//                    Event::EvReaderStopped => {
//                        // if reader stopped, the num_read is freezed.
//                        self.reader_stopped = true;
//                        self.num_read = *arg.downcast().expect("model:EvReaderStopped: failed to get argument");
//                    }
//
//                    Event::EvReaderStarted => {
//                        self.reader_stopped = false;
//                        self.num_read = 0;
//                    }
//
//                    //---------------------------------------------------------
//                    // Actions
//                    Event::EvActAccept => {
//                        curses.close();
//
//                        // output the expect key
//                        let tx_ack: Sender<Vec<Arc<Item>>> =
//                            *arg.downcast().expect("model:EvActAccept: failed to get argument");
//
//                        // do the final dirty work
//                        self.act_output();
//
//                        let mut selected: Vec<Arc<Item>> =
//                            self.selected.values().map(|item| item.item.clone()).collect();
//
//                        selected.sort_by_key(|item| item.get_full_index());
//
//                        // return the selected items
//                        let _ = tx_ack.send(selected);
//                        break;
//                    }
//                    Event::EvActAbort => {
//                        if let Some(tx_preview) = &self.tx_preview {
//                            tx_preview
//                                .send((
//                                    Event::EvActAbort,
//                                    PreviewInput {
//                                        cmd: "".into(),
//                                        lines: 0,
//                                        columns: 0,
//                                    },
//                                ))
//                                .expect("Failed to send to tx_preview");
//                        }
//                        let tx_ack: Sender<bool> = *arg.downcast().expect("model:EvActAbort: failed to get argument");
//                        curses.close();
//                        let _ = tx_ack.send(true);
//                        break;
//                    }
//                    Event::EvActUp => {
//                        //debug!("model:redraw_items_and_status");
//                        self.act_move_line_cursor(1);
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActDown => {
//                        //debug!("model:redraw_items_and_status");
//                        self.act_move_line_cursor(-1);
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActToggle => {
//                        //debug!("model:redraw_items_and_status");
//                        self.act_toggle();
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActToggleDown => {
//                        //debug!("model:redraw_items_and_status");
//                        self.act_toggle();
//                        self.act_move_line_cursor(-1);
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActToggleUp => {
//                        //debug!("model:redraw_items_and_status");
//                        self.act_toggle();
//                        self.act_move_line_cursor(1);
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActToggleAll => {
//                        //debug!("model:redraw_items_and_status");
//                        self.act_toggle_all();
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActSelectAll => {
//                        //debug!("model:redraw_items_and_status");
//                        self.act_select_all();
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActDeselectAll => {
//                        //debug!("model:redraw_items_and_status");
//                        self.act_deselect_all();
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActPageDown => {
//                        //debug!("model:redraw_items_and_status");
//                        let height = 1 - (self.height as i32);
//                        self.act_move_line_cursor(height);
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActPageUp => {
//                        //debug!("model:redraw_items_and_status");
//                        let height = (self.height as i32) - 1;
//                        self.act_move_line_cursor(height);
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActScrollLeft => {
//                        //debug!("model:redraw_items_and_status");
//                        self.act_scroll(*arg.downcast::<i32>().unwrap_or_else(|_| Box::new(-1)));
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//                    Event::EvActScrollRight => {
//                        //debug!("model:redraw_items_and_status");
//                        self.act_scroll(*arg.downcast::<i32>().unwrap_or_else(|_| Box::new(1)));
//                        self.act_redraw_items_and_status(&mut curses);
//                    }
//
//                    Event::EvActTogglePreview => {
//                        self.act_toggle_preview(&mut curses);
//                        // main loop will send EvActRedraw afterwards
//                        // so no need to call redraw here (besides, print_query_func is unknown)
//                    }
//
//                    Event::EvActRedraw => {
//                        //debug!("model:EvActRedraw:act_redraw");
//                        let print_query_func = *arg
//                            .downcast::<QueryPrintClosure>()
//                            .expect("model:EvActRedraw: failed to get argument");
//                        self.act_redraw(&mut curses, print_query_func);
//                    }
//                    _ => {}
//                }
//            }
//        }
//    }
//
//    fn clean_model(&mut self) {
//        self.items.clear();
//        self.item_cursor = 0;
//        self.line_cursor = 0;
//        self.hscroll_offset = 0;
//        self.matcher_stopped = false;
//        if !self.reader_stopped {
//            self.num_processed = 0;
//        }
//    }
//
//    fn update_size(&mut self, curses: &mut Window) {
//        // update the (height, width)
//        let (h, w) = curses.get_maxyx();
//        self.height = h - self.reserved_height;
//        self.width = w - 2;
//    }
//
//    fn insert_new_items(&mut self, items: MatchedItemGroup) {
//        for item in items {
//            self.items.push(Arc::new(item));
//        }
//    }
//
//    fn get_status_position(&self, cursor_y: usize) -> (usize, usize) {
//        match (self.inline_info, self.reverse) {
//            (false, true) => (1, 0),
//            (false, false) => ({ self.height + self.reserved_height - 2 }, 0),
//            (true, _) => (cursor_y, self.query_end_x),
//        }
//    }
//
//    fn draw_status(&self, curses: &mut Window) {
//        // cursor should be placed on query, so store cursor before printing
//        let (y, x) = curses.getyx();
//
//        let (status_y, status_x) = self.get_status_position(y);
//
//        curses.mv(status_y, status_x);
//        curses.clrtoeol();
//
//        if self.inline_info {
//            curses.print_with_attr("  <", self.theme.prompt());
//        };
//
//        // display spinner
//        if self.reader_stopped {
//            self.print_char(curses, ' ', self.theme.normal());
//        } else {
//            let time = self.timer.elapsed();
//            let mills = (time.as_secs() * 1000) as u32 + time.subsec_millis();
//            let index = (mills / SPINNER_DURATION) % (SPINNERS.len() as u32);
//            self.print_char(curses, SPINNERS[index as usize], self.theme.spinner());
//        }
//
//        // display matched/total number
//        curses.print_with_attr(
//            format!(" {}/{}", self.items.len(), self.num_read).as_ref(),
//            self.theme.info(),
//        );
//
//        // display the matcher mode
//        if !self.matcher_mode.is_empty() {
//            curses.print_with_attr(format!("/{}", &self.matcher_mode).as_ref(), self.theme.info());
//        }
//
//        // display the percentage of the number of processed items
//        if self.num_processed < self.num_read {
//            curses.print_with_attr(
//                format!(" ({}%) ", self.num_processed * 100 / self.num_read).as_ref(),
//                self.theme.info(),
//            )
//        }
//
//        // selected number
//        if self.multi_selection && !self.selected.is_empty() {
//            curses.print_with_attr(
//                format!(" [{}]", self.selected.len()).as_ref(),
//                Attr {
//                    effect: Effect::BOLD,
//                    ..self.theme.info()
//                },
//            );
//        }
//
//        // item cursor
//        let line_num_str = format!(" {} ", self.item_cursor + self.line_cursor);
//        curses.mv(status_y, self.width - line_num_str.len());
//        curses.print_with_attr(
//            &line_num_str,
//            Attr {
//                effect: Effect::BOLD,
//                ..self.theme.info()
//            },
//        );
//
//        // restore cursor
//        curses.mv(y, x);
//    }
//
//    fn get_header_height(&self, query_y: usize, maxy: usize) -> Option<usize> {
//        let (status_height, _) = self.get_status_position(query_y);
//        let res = if self.reverse {
//            status_height + 1
//        } else {
//            status_height - 1
//        };
//
//        if self.reserved_height + 1 < maxy && maxy > 3 {
//            Some(res)
//        } else {
//            None
//        }
//    }
//
//    fn draw_headers(&self, curses: &mut Window) {
//        // cursor should be placed on query, so store cursor before printing
//        let (y, x) = curses.getyx();
//        let (maxy, _) = curses.get_maxyx();
//        let (has_headers, yh) = (!self.headers.is_empty(), self.get_header_height(y, maxy));
//        if !has_headers || yh.is_none() {
//            return;
//        }
//        let yh = yh.unwrap();
//        let direction = if self.reverse { 1 as i64 } else { -1 };
//
//        let mut printer = LinePrinter::builder()
//            .container_width(self.width as usize)
//            .shift(0)
//            .hscroll_offset(self.hscroll_offset)
//            .build();
//
//        for (i, header) in self.headers.iter().enumerate() {
//            let nyh = ((yh as i64) + direction * (i as i64)) as usize;
//            curses.mv(nyh, 0);
//            curses.clrtoeol();
//            curses.mv(nyh, 2);
//            for (ch, attr) in header.iter() {
//                printer.print_char(curses, ch, self.theme.normal().extend(attr), false);
//            }
//        }
//        // restore cursor
//        curses.mv(y, x);
//    }
//
//    pub fn set_previewer(&mut self, tx_preview: Sender<(Event, PreviewInput)>) {
//        self.tx_preview = Some(tx_preview);
//    }
//
//    fn draw_preview(&mut self, curses: &mut Window) {
//        if self.preview_hidden {
//            return;
//        }
//
//        if self.preview_cmd.is_none() {
//            return;
//        }
//
//        // cursor should be placed on query, so store cursor before printing
//        let (lines, cols) = curses.get_maxyx();
//
//        let current_idx = self.item_cursor + self.line_cursor;
//        if current_idx >= self.items.len() {
//            curses.clrtoend();
//            return;
//        }
//
//        let item = Arc::clone(
//            self.items
//                .get(current_idx)
//                .unwrap_or_else(|| panic!("model:draw_items: failed to get item at {}", current_idx)),
//        );
//
//        let current_line = item.item.get_output_text();
//        let cmd = self.inject_preview_command(&current_line);
//
//        if let Some(tx_preview) = &self.tx_preview {
//            tx_preview
//                .send((
//                    Event::EvModelNewPreview,
//                    PreviewInput {
//                        cmd: cmd.to_string(),
//                        lines,
//                        columns: cols,
//                    },
//                ))
//                .expect("failed to send to previewer");
//        }
//    }
//
//    fn handle_preview_output(&mut self, curses: &mut Window, aoutput: AnsiString) {
//        debug!("model:draw_preview: output = {:?}", &aoutput);
//
//        curses.mv(0, 0);
//        for (ch, attr) in aoutput.iter() {
//            curses.add_char_with_attr(ch, attr);
//        }
//        curses.clrtoend();
//    }
//
//    fn inject_preview_command(&self, text: &str) -> Cow<str> {
//        let cmd = self
//            .preview_cmd
//            .as_ref()
//            .expect("model:inject_preview_command: invalid preview command");
//        debug!("replace: {:?}, text: {:?}", cmd, text);
//        RE_FIELDS.replace_all(cmd, |caps: &Captures| {
//            // \{...
//            if &caps[0][0..1] == "\\" {
//                return caps[0].to_string();
//            }
//
//            // {1..} and other variant
//            assert!(caps[1].len() >= 2);
//            let range = &caps[1][1..caps[1].len() - 1];
//            let replacement = if range == "" {
//                text
//            } else {
//                get_string_by_range(&self.delimiter, text, range).unwrap_or("")
//            };
//
//            format!("'{}'", escape_single_quote(replacement))
//        })
//    }
//
//    fn print_char(&self, curses: &mut Window, ch: char, attr: Attr) {
//        if ch != '\t' {
//            curses.add_char_with_attr(ch, attr);
//        } else {
//            // handle tabstop
//            let (_, x) = curses.getyx();
//            let rest = self.tabstop - (x as usize - 2) % self.tabstop;
//            for _ in 0..rest {
//                curses.add_char_with_attr(' ', attr);
//            }
//        }
//    }
//
//    //--------------------------------------------------------------------------
//    // Actions
//
//    pub fn act_move_line_cursor(&mut self, diff: i32) {
//        let diff = if self.reverse { -diff } else { diff };
//        let mut line_cursor = self.line_cursor as i32;
//        let mut item_cursor = self.item_cursor as i32;
//        let item_len = self.items.len() as i32;
//
//        let height = self.height as i32;
//
//        line_cursor += diff;
//        if line_cursor >= height {
//            item_cursor += line_cursor - height + 1;
//            item_cursor = max(0, min(item_cursor, item_len - height));
//            line_cursor = min(height - 1, item_len - item_cursor);
//        } else if line_cursor < 0 {
//            item_cursor += line_cursor;
//            item_cursor = max(item_cursor, 0);
//            line_cursor = 0;
//        } else {
//            line_cursor = max(0, min(line_cursor, item_len - 1 - item_cursor));
//        }
//
//        self.item_cursor = item_cursor as usize;
//        self.line_cursor = line_cursor as usize;
//    }
//
//    pub fn act_toggle(&mut self) {
//        if !self.multi_selection || self.items.is_empty() {
//            return;
//        }
//
//        let cursor = self.item_cursor + self.line_cursor;
//        let current_item = self
//            .items
//            .get(cursor)
//            .unwrap_or_else(|| panic!("model:act_toggle: failed to get item {}", cursor));
//        let index = current_item.item.get_full_index();
//        if !self.selected.contains_key(&index) {
//            self.selected.insert(index, Arc::clone(current_item));
//        } else {
//            self.selected.remove(&index);
//        }
//    }
//
//    pub fn act_toggle_all(&mut self) {
//        for current_item in self.items.iter() {
//            let index = current_item.item.get_full_index();
//            if !self.selected.contains_key(&index) {
//                self.selected.insert(index, Arc::clone(current_item));
//            } else {
//                self.selected.remove(&index);
//            }
//        }
//    }
//
//    pub fn act_select_all(&mut self) {
//        for current_item in self.items.iter() {
//            let index = current_item.item.get_full_index();
//            self.selected.insert(index, Arc::clone(current_item));
//        }
//    }
//
//    pub fn act_deselect_all(&mut self) {
//        self.selected.clear();
//    }
//
//    pub fn act_output(&mut self) {
//        // select the current one
//        if !self.items.is_empty() {
//            let cursor = self.item_cursor + self.line_cursor;
//            let current_item = self
//                .items
//                .get(cursor)
//                .unwrap_or_else(|| panic!("model:act_output: failed to get item {}", cursor));
//            let index = current_item.item.get_full_index();
//            self.selected.insert(index, Arc::clone(current_item));
//        }
//    }
//
//    pub fn act_toggle_preview(&mut self, curses: &mut Curses) {
//        self.preview_hidden = !self.preview_hidden;
//        curses.toggle_preview_window();
//    }
//
//    pub fn act_scroll(&mut self, offset: i32) {
//        let mut hscroll_offset = self.hscroll_offset as i32;
//        hscroll_offset += offset;
//        hscroll_offset = max(0, hscroll_offset);
//        self.hscroll_offset = hscroll_offset as usize;
//    }
//
//    pub fn act_redraw(&mut self, curses: &mut Curses, print_query_func: QueryPrintClosure) {
//        curses.resize();
//        self.update_size(&mut curses.win_main);
//        self.draw_preview(&mut curses.win_preview);
//        self.draw_items(&mut curses.win_main);
//        self.draw_query(&mut curses.win_main, &print_query_func);
//        self.draw_status(&mut curses.win_main);
//        self.draw_headers(&mut curses.win_main);
//        curses.refresh();
//    }
//
//    fn act_redraw_items_and_status(&mut self, curses: &mut Curses) {
//        curses.win_main.hide_cursor();
//        self.update_size(&mut curses.win_main);
//        self.draw_preview(&mut curses.win_preview);
//        self.draw_items(&mut curses.win_main);
//        self.draw_status(&mut curses.win_main);
//        curses.win_main.show_cursor();
//        curses.refresh();
//    }
}

impl Draw for Model {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        let (screen_width, screen_height) = canvas.size()?;

        let total = self.item_pool.len();
        let status = Status {
            total,
            matched: self.selection.num_options(),
            processed: self.matcher_control.as_ref().map(|c|c.get_num_processed()).unwrap_or(total),
            multi_selection: self.selection.is_multi_selection(),
            selected: self.selection.get_num_selected(),
            current_item_idx: self.selection.get_current_item_idx(),
            reading: !self.reader_control.as_ref().map(|c| c.is_processed()).unwrap_or(true),
            time: self.timer.elapsed(),
            matcher_mode: "".to_string(),
            theme: self.theme.clone(),
        };

        let win_selection = Win::new(&self.selection);
        let win_query = Win::new(&self.query)
            .basis(1.into())
            .grow(0)
            .shrink(0);
        let win_status = Win::new(&status)
            .basis(1.into())
            .grow(0)
            .shrink(0);

        let screen = if self.reverse {
            VSplit::default()
                .split(&win_query)
                .split(&win_status)
                .split(&win_selection)
        } else {
            VSplit::default()
                .split(&win_selection)
                .split(&win_status)
                .split(&win_query)
        };

        screen.draw(canvas)
    }
}

struct Status {
    total: usize,
    matched: usize,
    processed: usize,
    multi_selection: bool,
    selected: usize,
    current_item_idx: usize,
    reading: bool,
    time: Duration,
    matcher_mode: String,
    theme: Arc<ColorTheme>,
}

impl Draw for Status {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        canvas.clear()?;
        let (screen_width, _) = canvas.size()?;
        if self.reading {
            let mills = (self.time.as_secs() * 1000) as u32 + self.time.subsec_millis();
            let index = (mills / SPINNER_DURATION) % (SPINNERS.len() as u32);
            canvas.put_cell(0, 0, Cell{ch: SPINNERS[index as usize], attr: self.theme.spinner()});
        }

        let info_attr = self.theme.info();
        let info_attr_bold = Attr{effect: Effect::BOLD, ..self.theme.info()};

        // display matched/total number
        let mut col = 1;
        col += canvas.print_with_attr(0, col, format!(" {}/{}", self.matched, self.total).as_ref(), info_attr)?;

        // display the matcher mode
        if !self.matcher_mode.is_empty() {
            col += canvas.print_with_attr(0, col, format!("/{}", &self.matcher_mode).as_ref(), info_attr)?;
        }

        // display the percentage of the number of processed items
        if self.processed < self.total{
            col += canvas.print_with_attr(0, col, format!(" ({}%) ", self.processed * 100 / self.total).as_ref(), info_attr)?;
        }

        // selected number
        if self.multi_selection && self.selected > 0 {
            col += canvas.print_with_attr(0, col, format!(" [{}]", self.selected).as_ref(), info_attr_bold)?;
        }

        // item cursor
        let line_num_str = format!(" {} ", self.current_item_idx);
        canvas.print_with_attr(0, screen_width - line_num_str.len(), &line_num_str, info_attr_bold)?;

        Ok(())
    }
}

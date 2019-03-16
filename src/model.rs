use crate::ansi::AnsiString;
use crate::event::{Event, EventHandler, EventReceiver, EventSender, UpdateScreen};
use crate::field::get_string_by_range;
use crate::header::Header;
use crate::item::{Item, ItemPool};
use crate::matcher::{Matcher, MatcherControl, MatcherMode};
use crate::options::SkimOptions;
use crate::output::SkimOutput;
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
use std::env;
use std::mem;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tuikit::prelude::*;
use unicode_width::UnicodeWidthChar;

const SPINNER_DURATION: u32 = 200;
const SPINNERS: [char; 8] = ['-', '\\', '|', '/', '-', '\\', '|', '/'];
const DELIMITER_STR: &str = r"[\t\n ]+";

lazy_static! {
    static ref RE_FIELDS: Regex = Regex::new(r"\\?(\{-?[0-9.,q]*?})").unwrap();
    static ref REFRESH_DURATION: Duration = Duration::from_millis(50);
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

    tx_preview: Option<Sender<(Event, PreviewInput)>>,
    header: Header,

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

            tx_preview: None,
            header: Header::empty(),

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

        self.header = Header::with_options(options);
    }

    pub fn start(&mut self) -> Option<SkimOutput> {
        let mut cmd = self.query.get_cmd();
        let mut query = self.query.get_query();
        let mut to_clear_selection = false;

        self.reader_control = Some(self.reader.run(&cmd));

        while let Ok((ev, arg)) = self.rx.recv() {
            debug!("model: ev: {:?}, arg: {:?}", ev, arg);

            if self.header.accept_event(ev) {
                self.header.handle(ev, &arg);
            }

            if self.query.accept_event(ev) {
                self.query.handle(ev, &arg);
                let new_query = self.query.get_query();
                let new_cmd = self.query.get_cmd();

                // re-run reader & matcher if needed;
                if new_cmd != cmd {
                    cmd = new_cmd;

                    // stop matcher
                    self.reader_control.take().map(ReaderControl::kill);
                    self.matcher_control.take().map(|ctrl: MatcherControl| ctrl.kill());
                    self.item_pool.clear();
                    to_clear_selection = true;

                    // restart reader
                    self.reader_control.replace(self.reader.run(&cmd));
                    self.restart_matcher();
                } else if query != new_query {
                    query = new_query;

                    // restart matcher
                    self.matcher_control.take().map(|ctrl| ctrl.kill());
                    to_clear_selection = true;
                    self.item_pool.reset();
                    self.restart_matcher();
                }
            }

            if self.selection.accept_event(ev) {
                self.selection.handle(ev, &arg);
            }

            match ev {
                Event::EvHeartBeat => {
                    // save the processed items
                    if self
                        .matcher_control
                        .as_ref()
                        .map(|ctrl| ctrl.stopped())
                        .unwrap_or(false)
                    {
                        self.matcher_control.take().map(|ctrl| {
                            let lock = ctrl.into_items();
                            let mut items = lock.lock();
                            let matched = mem::replace(&mut *items, Vec::new());

                            if to_clear_selection {
                                to_clear_selection = false;
                                self.selection.clear();
                            }

                            self.selection.append_sorted_items(matched);
                        });
                    }

                    let processed = self.reader_control.as_ref().map(|c| c.is_processed()).unwrap_or(true);
                    // run matcher if matcher had been stopped and reader had new items.
                    if !processed && self.matcher_control.is_none() {
                        self.restart_matcher();
                    }
                }

                Event::EvActAccept => {
                    debug!("accept");
                    self.reader_control.take().map(|ctrl| ctrl.kill());
                    self.matcher_control.take().map(|ctrl| ctrl.kill());
                    debug!("threads killed");

                    return Some(SkimOutput {
                        accept_key: None,
                        query: self.query.get_query(),
                        cmd: self.query.get_cmd_query(),
                        selected_items: self.selection.get_selected_items(),
                    });
                }

                _ => {}
            }

            self.term.draw(self);
            self.term.present();
        }

        None
    }

    fn restart_matcher(&mut self) {
        let query = self.query.get_query();

        // kill existing matcher if exits
        self.matcher_control.take().map(|ctrl| ctrl.kill());

        // if there are new items, move them to item pool
        let processed = self.reader_control.as_ref().map(|c| c.is_processed()).unwrap_or(true);
        if !processed {
            // take out new items and put them into items
            let mut new_items = self.reader_control.as_ref().map(|c| c.take()).unwrap();
            self.item_pool.append(&mut new_items);
        };

        let tx_clone = self.tx.clone();
        self.matcher_control
            .replace(self.matcher.run(&query, self.item_pool.clone(), None));
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
    //                    Event::EvModelNewPreview => {
    //                        //debug!("model:EvModelNewPreview:handle_preview_output");
    //                        let preview_output = *arg
    //                            .downcast::<AnsiString>()
    //                            .expect("model:EvModelNewPreview: failed to get argument");
    //                        self.handle_preview_output(&mut curses.win_preview, preview_output);
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
    //
    //                    Event::EvActTogglePreview => {
    //                        self.act_toggle_preview(&mut curses);
    //                        // main loop will send EvActRedraw afterwards
    //                        // so no need to call redraw here (besides, print_query_func is unknown)
    //                    }
    //
    //                    _ => {}
    //                }
    //            }
    //        }
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
    //    pub fn act_toggle_preview(&mut self, curses: &mut Curses) {
    //        self.preview_hidden = !self.preview_hidden;
    //        curses.toggle_preview_window();
    //    }
}

impl Draw for Model {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        let (screen_width, screen_height) = canvas.size()?;

        debug!("prepare status, {}", self.matcher_control.is_some());
        let total = self.item_pool.len();
        let status = Status {
            total,
            matched: self.selection.num_options()
                + self.matcher_control.as_ref().map(|c| c.get_num_matched()).unwrap_or(0),
            processed: self
                .matcher_control
                .as_ref()
                .map(|c| c.get_num_processed())
                .unwrap_or(total),
            matcher_running: self.matcher_control.is_some(),
            multi_selection: self.selection.is_multi_selection(),
            selected: self.selection.get_num_selected(),
            current_item_idx: self.selection.get_current_item_idx(),
            reading: !self.reader_control.as_ref().map(|c| c.is_processed()).unwrap_or(true),
            time: self.timer.elapsed(),
            matcher_mode: "".to_string(),
            theme: self.theme.clone(),
            inline_info: self.inline_info,
        };
        debug!("prepare done");

        let win_selection = Win::new(&self.selection);
        let win_query = Win::new(&self.query)
            .basis(if self.inline_info { 0 } else { 1 }.into())
            .grow(0)
            .shrink(0);
        let win_status = Win::new(&status)
            .basis(if self.inline_info { 0 } else { 1 }.into())
            .grow(0)
            .shrink(0);
        let win_header = Win::new(&self.header)
            .basis(if self.header.is_empty() { 0 } else { 1 }.into())
            .grow(0)
            .shrink(0);
        let win_query_status = HSplit::default()
            .basis(if self.inline_info { 1 } else { 0 }.into())
            .grow(0)
            .shrink(0)
            .split(Win::new(&self.query).grow(0).shrink(0))
            .split(Win::new(&status).grow(1).shrink(0));

        let screen = if self.reverse {
            VSplit::default()
                .split(&win_query_status)
                .split(&win_query)
                .split(&win_status)
                .split(&win_header)
                .split(&win_selection)
        } else {
            VSplit::default()
                .split(&win_selection)
                .split(&win_header)
                .split(&win_status)
                .split(&win_query)
                .split(&win_query_status)
        };

        screen.draw(canvas)
    }
}

struct Status {
    total: usize,
    matched: usize,
    processed: usize,
    matcher_running: bool,
    multi_selection: bool,
    selected: usize,
    current_item_idx: usize,
    reading: bool,
    time: Duration,
    matcher_mode: String,
    theme: Arc<ColorTheme>,
    inline_info: bool,
}

impl Draw for Status {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        canvas.clear()?;
        let (screen_width, _) = canvas.size()?;

        let mut col = 0;

        if self.inline_info {
            col += canvas.print_with_attr(0, col, " <", self.theme.prompt())?;
        }

        if self.reading {
            let mills = (self.time.as_secs() * 1000) as u32 + self.time.subsec_millis();
            let index = (mills / SPINNER_DURATION) % (SPINNERS.len() as u32);
            let ch = SPINNERS[index as usize];
            col += canvas.print_with_attr(0, col, &ch.to_string(), self.theme.spinner())?;
        }

        let info_attr = self.theme.info();
        let info_attr_bold = Attr {
            effect: Effect::BOLD,
            ..self.theme.info()
        };

        // display matched/total number
        col += canvas.print_with_attr(0, col, format!(" {}/{}", self.matched, self.total).as_ref(), info_attr)?;

        // display the matcher mode
        if !self.matcher_mode.is_empty() {
            col += canvas.print_with_attr(0, col, format!("/{}", &self.matcher_mode).as_ref(), info_attr)?;
        }

        // display the percentage of the number of processed items
        if self.matcher_running && self.processed * 20 > self.total {
            col += canvas.print_with_attr(
                0,
                col,
                format!(" ({}%) ", self.processed * 100 / self.total).as_ref(),
                info_attr,
            )?;
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

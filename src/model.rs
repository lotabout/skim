use crate::ansi::AnsiString;
use crate::event::{Event, EventHandler, EventReceiver, EventSender, UpdateScreen};
use crate::field::get_string_by_range;
use crate::header::Header;
use crate::item::{Item, ItemPool};
use crate::matcher::{Matcher, MatcherControl, MatcherMode};
use crate::options::SkimOptions;
use crate::output::SkimOutput;
use crate::previewer::Previewer;
use crate::query::Query;
use crate::reader::{Reader, ReaderControl};
use crate::selection::Selection;
use crate::spinlock::SpinLock;
use crate::theme::{ColorTheme, DEFAULT_THEME};
use crate::util::{escape_single_quote, margin_string_to_size};
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

    header: Header,

    preview_hidden: bool,
    previewer: Option<Previewer>,
    preview_direction: Direction,
    preview_size: Size,

    // Options
    reverse: bool,
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

            header: Header::empty(),
            preview_hidden: true,
            previewer: None,
            preview_direction: Direction::Right,
            preview_size: Size::Default,

            reverse: false,
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

        // preview related
        let (preview_direction, preview_size, preview_wrap, preview_shown) = options
            .preview_window
            .map(Self::parse_preview)
            .expect("option 'preview-window' should be set (by default)");
        self.preview_direction = preview_direction;
        self.preview_size = preview_size;
        self.preview_hidden = !preview_shown;

        if let Some(preview_cmd) = options.preview {
            self.previewer = Some(
                Previewer::new(Some(preview_cmd.to_string()))
                    .wrap(preview_wrap)
                    .delimiter(self.delimiter.clone()),
            );
        }
    }

    // -> (direction, size, wrap, shown)
    fn parse_preview(preview_option: &str) -> (Direction, Size, bool, bool) {
        let options = preview_option.split(':').collect::<Vec<&str>>();

        let mut direction = Direction::Right;
        let mut shown = true;
        let mut wrap = false;
        let mut size = Size::Percent(50);

        for option in options {
            // mistake
            if option.is_empty() {
                continue;
            }

            let first_char = option.chars().next().unwrap_or('A');

            // raw string
            if first_char.is_digit(10) {
                size = margin_string_to_size(option);
            } else {
                match option.to_uppercase().as_str() {
                    "UP" => direction = Direction::Up,
                    "DOWN" => direction = Direction::Down,
                    "LEFT" => direction = Direction::Left,
                    "RIGHT" => direction = Direction::Right,
                    "HIDDEN" => shown = false,
                    "WRAP" => wrap = true,
                    _ => {}
                }
            }
        }

        (direction, size, wrap, shown)
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
                    self.reader_control.take().map(|ctrl| ctrl.kill());
                    self.matcher_control.take().map(|ctrl| ctrl.kill());

                    return Some(SkimOutput {
                        accept_key: None,
                        query: self.query.get_query(),
                        cmd: self.query.get_cmd_query(),
                        selected_items: self.selection.get_selected_items(),
                    });
                }

                Event::EvActAbort => {
                    self.reader_control.take().map(|ctrl| ctrl.kill());
                    self.matcher_control.take().map(|ctrl| ctrl.kill());
                    return None;
                }

                Event::EvActTogglePreview => {
                    self.preview_hidden = !self.preview_hidden;
                }

                _ => {}
            }

            if !self.preview_hidden {
                let item = self.selection.get_current_item();
                if item.is_some() {
                    let item = item.unwrap();
                    self.previewer.as_mut().map(|p| p.on_item_change(item));
                }
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
    //                    _ => {}
    //                }
    //            }
    //        }
    //    }
}

impl Draw for Model {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        let (screen_width, screen_height) = canvas.size()?;

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

        let win_main = if self.reverse {
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

        let screen: Box<dyn Draw> = if !self.preview_hidden && self.previewer.is_some() {
            let previewer = self.previewer.as_ref().unwrap();
            let win = Win::new(previewer)
                .basis(self.preview_size)
                .grow(0)
                .shrink(0)
                .border_attr(self.theme.border());

            let win_preview = match self.preview_direction {
                Direction::Up => win.border_bottom(true),
                Direction::Right => win.border_left(true),
                Direction::Down => win.border_top(true),
                Direction::Left => win.border_right(true),
            };

            match self.preview_direction {
                Direction::Up => Box::new(VSplit::default().split(win_preview).split(win_main)),
                Direction::Right => Box::new(HSplit::default().split(win_main).split(win_preview)),
                Direction::Down => Box::new(VSplit::default().split(win_main).split(win_preview)),
                Direction::Left => Box::new(HSplit::default().split(win_preview).split(win_main)),
            }
        } else {
            Box::new(win_main)
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
            col += canvas.put_char_with_attr(0, col, ch, self.theme.spinner())?;
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

#[derive(PartialEq, Eq, Clone, Debug, Copy)]
enum Direction {
    Up,
    Down,
    Left,
    Right,
}

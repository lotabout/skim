use crate::event::{Event, EventHandler, EventReceiver, EventSender};
use crate::header::Header;
use crate::item::ItemPool;
use crate::matcher::{Matcher, MatcherControl, MatcherMode};
use crate::options::SkimOptions;
use crate::output::SkimOutput;
use crate::previewer::Previewer;
use crate::query::Query;
use crate::reader::{Reader, ReaderControl};
use crate::selection::Selection;
use crate::theme::ColorTheme;
use crate::util::margin_string_to_size;
use regex::Regex;
use std::env;
use std::mem;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tuikit::prelude::{Event as TermEvent, *};

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

    matcher_mode: Option<MatcherMode>,
    reader_timer: Instant,
    matcher_timer: Instant,
    reader_control: Option<ReaderControl>,
    matcher_control: Option<MatcherControl>,

    header: Header,

    preview_hidden: bool,
    previewer: Option<Previewer>,
    preview_direction: Direction,
    preview_size: Size,

    // Options
    layout: String,
    delimiter: Regex,
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
        let item_pool = Arc::new(ItemPool::new().lines_to_reserve(options.header_lines));
        let header = Header::empty().with_options(options).item_pool(item_pool.clone());

        let mut ret = Model {
            reader,
            query,
            selection,
            matcher,
            term,
            item_pool,

            rx,
            tx,
            reader_timer: Instant::now(),
            matcher_timer: Instant::now(),
            reader_control: None,
            matcher_control: None,
            matcher_mode: None,

            header,
            preview_hidden: true,
            previewer: None,
            preview_direction: Direction::Right,
            preview_size: Size::Default,

            layout: "default".to_string(),
            delimiter: Regex::new(DELIMITER_STR).unwrap(),
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

        self.layout = options.layout.to_string();

        if options.inline_info {
            self.inline_info = true;
        }

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

    fn act_heart_beat(&mut self, env: &mut ModelEnv) {
        // save the processed items
        let matcher_stopped = self
            .matcher_control
            .as_ref()
            .map(|ctrl| ctrl.stopped())
            .unwrap_or(false);

        if matcher_stopped {
            let ctrl = self.matcher_control.take().unwrap();
            let lock = ctrl.into_items();
            let mut items = lock.lock();
            let matched = mem::replace(&mut *items, Vec::new());

            match env.clear_selection {
                ClearStrategy::DontClear => {}
                ClearStrategy::Clear => {
                    self.selection.clear();
                    env.clear_selection = ClearStrategy::DontClear;
                }
                ClearStrategy::ClearIfNotNull => {
                    if !matched.is_empty() {
                        self.selection.clear();
                        env.clear_selection = ClearStrategy::DontClear;
                    }
                }
            };
            self.selection.append_sorted_items(matched);
        }

        let processed = self.reader_control.as_ref().map(|c| c.is_processed()).unwrap_or(true);
        // run matcher if matcher had been stopped and reader had new items.
        if !processed && self.matcher_control.is_none() {
            self.restart_matcher();
        }
    }

    fn act_rotate_mode(&mut self, env: &mut ModelEnv) {
        if self.matcher_mode.is_none() {
            self.matcher_mode = Some(MatcherMode::Regex);
        } else {
            self.matcher_mode = None;
        }

        // restart matcher
        if let Some(ctrl) = self.matcher_control.take() {
            ctrl.kill();
        }

        env.clear_selection = ClearStrategy::Clear;
        self.item_pool.reset();
        self.restart_matcher();
    }

    fn on_cmd_query_change(&mut self, env: &mut ModelEnv) {
        // stop matcher
        if let Some(ctrl) = self.reader_control.take() {
            ctrl.kill();
        }
        if let Some(ctrl) = self.matcher_control.take() {
            ctrl.kill();
        }

        self.item_pool.clear();
        env.clear_selection = ClearStrategy::ClearIfNotNull;

        // restart reader
        self.reader_control.replace(self.reader.run(&env.cmd));
        self.restart_matcher();
        self.reader_timer = Instant::now();
    }

    fn on_query_change(&mut self, env: &mut ModelEnv) {
        // restart matcher
        if let Some(ctrl) = self.matcher_control.take() {
            ctrl.kill();
        }
        env.clear_selection = ClearStrategy::Clear;
        self.item_pool.reset();
        self.restart_matcher();
    }

    pub fn start(&mut self) -> Option<SkimOutput> {
        let mut env = ModelEnv {
            cmd: self.query.get_cmd(),
            query: self.query.get_query(),
            clear_selection: ClearStrategy::DontClear,
        };

        self.reader_control = Some(self.reader.run(&env.cmd));

        while let Ok((ev, arg)) = self.rx.recv() {
            debug!("model: ev: {:?}, arg: {:?}", ev, arg);
            match ev {
                Event::EvHeartBeat => {
                    self.act_heart_beat(&mut env);
                }

                Event::EvActTogglePreview => {
                    self.preview_hidden = !self.preview_hidden;
                }

                Event::EvActRotateMode => {
                    self.act_rotate_mode(&mut env);
                }

                Event::EvActAccept => {
                    let accept_key = arg.downcast_ref::<Option<String>>().and_then(|os| os.as_ref().cloned());

                    if let Some(ctrl) = self.reader_control.take() {
                        ctrl.kill();
                    }
                    if let Some(ctrl) = self.matcher_control.take() {
                        ctrl.kill();
                    }

                    return Some(SkimOutput {
                        accept_key,
                        query: self.query.get_query(),
                        cmd: self.query.get_cmd_query(),
                        selected_items: self.selection.get_selected_items(),
                    });
                }

                Event::EvActAbort => {
                    if let Some(ctrl) = self.reader_control.take() {
                        ctrl.kill();
                    }
                    if let Some(ctrl) = self.matcher_control.take() {
                        ctrl.kill();
                    }
                    return None;
                }

                Event::EvActDeleteCharEOF => {
                    if env.query.is_empty() {
                        let _ = self.term.send_event(TermEvent::Key(Key::Null));
                        if let Some(ctrl) = self.reader_control.take() {
                            ctrl.kill();
                        }
                        if let Some(ctrl) = self.matcher_control.take() {
                            ctrl.kill();
                        }
                        return None;
                    }
                }

                _ => {}
            }

            // dispatch events to sub-components

            if self.header.accept_event(ev) {
                self.header.handle(ev, &arg);
            }

            if self.query.accept_event(ev) {
                self.query.handle(ev, &arg);
                let new_query = self.query.get_query();
                let new_cmd = self.query.get_cmd();

                // re-run reader & matcher if needed;
                if new_cmd != env.cmd {
                    env.cmd = new_cmd;
                    self.on_cmd_query_change(&mut env);
                } else if new_query != env.query {
                    env.query = new_query;
                    self.on_query_change(&mut env);
                }
            }

            if self.selection.accept_event(ev) {
                self.selection.handle(ev, &arg);
            }

            // re-draw
            if !self.preview_hidden {
                let item = self.selection.get_current_item();
                if item.is_some() {
                    let item = item.unwrap();
                    if let Some(previewer) = self.previewer.as_mut() {
                        previewer.on_item_change(item);
                    }
                }
            }

            let _ = self.term.draw(self);
            let _ = self.term.present();
        }

        None
    }

    fn restart_matcher(&mut self) {
        self.matcher_timer = Instant::now();
        let query = self.query.get_query();

        // kill existing matcher if exits
        if let Some(ctrl) = self.matcher_control.take() {
            ctrl.kill();
        }

        // if there are new items, move them to item pool
        let processed = self.reader_control.as_ref().map(|c| c.is_processed()).unwrap_or(true);
        if !processed {
            // take out new items and put them into items
            let new_items = self.reader_control.as_ref().map(|c| c.take()).unwrap();
            self.item_pool.append(new_items);
        };

        let _tx_clone = self.tx.clone();
        self.matcher_control
            .replace(self.matcher.run(&query, self.item_pool.clone(), self.matcher_mode));
    }
}

struct ModelEnv {
    pub cmd: String,
    pub query: String,
    pub clear_selection: ClearStrategy,
}

impl Draw for Model {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        let (_screen_width, _screen_height) = canvas.size()?;

        let total = self.item_pool.len();
        let matcher_mode = if self.matcher_mode.is_none() {
            "".to_string()
        } else {
            "RE".to_string()
        };

        let matched =
            self.selection.num_options() + self.matcher_control.as_ref().map(|c| c.get_num_matched()).unwrap_or(0);
        let processed = self
            .matcher_control
            .as_ref()
            .map(|c| c.get_num_processed())
            .unwrap_or(total);

        let status = Status {
            total,
            matched,
            processed,
            matcher_running: self.matcher_control.is_some(),
            multi_selection: self.selection.is_multi_selection(),
            selected: self.selection.get_num_selected(),
            current_item_idx: self.selection.get_current_item_idx(),
            reading: !self.reader_control.as_ref().map(|c| c.is_processed()).unwrap_or(true),
            time_since_read: self.reader_timer.elapsed(),
            time_since_match: self.matcher_timer.elapsed(),
            matcher_mode,
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
        let win_header = Win::new(&self.header).grow(0).shrink(0);
        let win_query_status = HSplit::default()
            .basis(if self.inline_info { 1 } else { 0 }.into())
            .grow(0)
            .shrink(0)
            .split(Win::new(&self.query).grow(0).shrink(0))
            .split(Win::new(&status).grow(1).shrink(0));

        let layout = &self.layout as &str;
        let win_main = match layout {
            "reverse" => VSplit::default()
                .split(&win_query_status)
                .split(&win_query)
                .split(&win_status)
                .split(&win_header)
                .split(&win_selection),
            "reverse-list" => VSplit::default()
                .split(&win_selection)
                .split(&win_header)
                .split(&win_status)
                .split(&win_query)
                .split(&win_query_status),
            _ => VSplit::default()
                .split(&win_selection)
                .split(&win_header)
                .split(&win_status)
                .split(&win_query)
                .split(&win_query_status),
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
    time_since_read: Duration,
    time_since_match: Duration,
    matcher_mode: String,
    theme: Arc<ColorTheme>,
    inline_info: bool,
}

#[allow(unused_assignments)]
impl Draw for Status {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        canvas.clear()?;
        let (screen_width, _) = canvas.size()?;

        let info_attr = self.theme.info();
        let info_attr_bold = Attr {
            effect: Effect::BOLD,
            ..self.theme.info()
        };

        let a_while_since_read = self.time_since_read > Duration::from_millis(200);
        let a_while_since_match = self.time_since_match > Duration::from_millis(200);

        let mut col = 0;
        if self.inline_info {
            col += canvas.print_with_attr(0, col, " <", self.theme.prompt())?;
        } else {
            // draw the spinner
            if self.reading && a_while_since_read {
                let mills = (self.time_since_read.as_secs() * 1000) as u32 + self.time_since_read.subsec_millis();
                let index = (mills / SPINNER_DURATION) % (SPINNERS.len() as u32);
                let ch = SPINNERS[index as usize];
                col += canvas.put_char_with_attr(0, col, ch, self.theme.spinner())?;
            } else {
                col += canvas.put_char_with_attr(0, col, ' ', info_attr)?;
            }
        }

        // display matched/total number
        col += canvas.print_with_attr(0, col, format!(" {}/{}", self.matched, self.total).as_ref(), info_attr)?;

        // display the matcher mode
        if !self.matcher_mode.is_empty() {
            col += canvas.print_with_attr(0, col, format!("/{}", &self.matcher_mode).as_ref(), info_attr)?;
        }

        // display the percentage of the number of processed items
        if self.matcher_running && a_while_since_match {
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

#[derive(PartialEq, Eq, Clone, Debug, Copy)]
enum ClearStrategy {
    DontClear,
    Clear,
    ClearIfNotNull,
}

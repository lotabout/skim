use std::borrow::Cow;
use std::cmp::{max, min};
use std::env;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;

use derive_builder::Builder;
use nix::libc;
use regex::Regex;
use tuikit::prelude::{Event as TermEvent, *};

use crate::ansi::{ANSIParser, AnsiString};
use crate::event::{Event, EventHandler, UpdateScreen};
use crate::spinlock::SpinLock;
use crate::util::{atoi, clear_canvas, depends_on_items, inject_command, InjectContext};
use crate::{ItemPreview, PreviewContext, PreviewPosition, SkimItem};

const TAB_STOP: usize = 8;
const DELIMITER_STR: &str = r"[\t\n ]+";

pub struct Previewer {
    tx_preview: Sender<PreviewEvent>,
    content_lines: Arc<SpinLock<Vec<AnsiString<'static>>>>,

    width: Arc<AtomicUsize>,
    height: Arc<AtomicUsize>,
    hscroll_offset: Arc<AtomicUsize>,
    vscroll_offset: Arc<AtomicUsize>,
    wrap: bool,

    prev_item: Option<Arc<dyn SkimItem>>,
    prev_query: Option<String>,
    prev_cmd_query: Option<String>,
    prev_num_selected: usize,

    preview_cmd: Option<String>,
    preview_offset: String, // e.g. +SCROLL-OFFSET
    delimiter: Regex,
    thread_previewer: Option<JoinHandle<()>>,
}

impl Previewer {
    pub fn new<C>(preview_cmd: Option<String>, callback: C) -> Self
    where
        C: Fn() + Send + Sync + 'static,
    {
        let content_lines = Arc::new(SpinLock::new(Vec::new()));
        let (tx_preview, rx_preview) = channel();
        let width = Arc::new(AtomicUsize::new(80));
        let height = Arc::new(AtomicUsize::new(60));
        let hscroll_offset = Arc::new(AtomicUsize::new(1));
        let vscroll_offset = Arc::new(AtomicUsize::new(1));

        let content_clone = content_lines.clone();
        let width_clone = width.clone();
        let height_clone = height.clone();
        let hscroll_offset_clone = hscroll_offset.clone();
        let vscroll_offset_clone = vscroll_offset.clone();
        let thread_previewer = thread::spawn(move || {
            run(rx_preview, move |lines, pos| {
                let width = width_clone.load(Ordering::SeqCst);
                let height = height_clone.load(Ordering::SeqCst);

                let hscroll = pos.h_scroll.calc_fixed_size(lines.len(), 0);
                let hoffset = pos.h_offset.calc_fixed_size(width, 0);
                let vscroll = pos.v_scroll.calc_fixed_size(usize::MAX, 0);
                let voffset = pos.v_offset.calc_fixed_size(height, 0);

                hscroll_offset_clone.store(max(1, max(hscroll, hoffset) - hoffset), Ordering::SeqCst);
                vscroll_offset_clone.store(max(1, max(vscroll, voffset) - voffset), Ordering::SeqCst);
                *content_clone.lock() = lines;

                callback();
            })
        });

        Self {
            tx_preview,
            content_lines,

            width,
            height,
            hscroll_offset,
            vscroll_offset,
            wrap: false,

            prev_item: None,
            prev_query: None,
            prev_cmd_query: None,
            prev_num_selected: 0,

            preview_cmd,
            preview_offset: "".to_string(),
            delimiter: Regex::new(DELIMITER_STR).unwrap(),
            thread_previewer: Some(thread_previewer),
        }
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    pub fn delimiter(mut self, delimiter: Regex) -> Self {
        self.delimiter = delimiter;
        self
    }

    // e.g. +SCROLL-OFFSET
    pub fn preview_offset(mut self, offset: String) -> Self {
        self.preview_offset = offset;
        self
    }

    #[allow(clippy::too_many_arguments)]
    pub fn on_item_change(
        &mut self,
        new_item_index: usize,
        new_item: impl Into<Option<Arc<dyn SkimItem>>>,
        new_query: impl Into<Option<String>>,
        new_cmd_query: impl Into<Option<String>>,
        num_selected: usize,
        get_selected_items: impl Fn() -> (Vec<usize>, Vec<Arc<dyn SkimItem>>), // lazy get
        force: bool,
    ) {
        let new_item = new_item.into();
        let new_query = new_query.into();
        let new_cmd_query = new_cmd_query.into();

        let item_changed = match (self.prev_item.as_ref(), new_item.as_ref()) {
            (None, None) => false,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            #[allow(clippy::vtable_address_comparisons)]
            (Some(prev), Some(new)) => !Arc::ptr_eq(prev, new),
        };

        let query_changed = match (self.prev_query.as_ref(), new_query.as_ref()) {
            (None, None) => false,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (Some(prev), Some(cur)) => prev != cur,
        };

        let cmd_query_changed = match (self.prev_cmd_query.as_ref(), new_cmd_query.as_ref()) {
            (None, None) => false,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (Some(prev), Some(cur)) => prev != cur,
        };

        let selected_items_changed = self.prev_num_selected != num_selected;

        if !force && !item_changed && !query_changed && !cmd_query_changed && !selected_items_changed {
            return;
        }

        self.prev_item = new_item.clone();
        self.prev_query = new_query;
        self.prev_cmd_query = new_cmd_query;
        self.prev_num_selected = num_selected;

        // prepare preview context

        let current_selection = self
            .prev_item
            .as_ref()
            .map(|item| item.output())
            .unwrap_or_else(|| "".into());
        let query = self.prev_query.as_deref().unwrap_or("");
        let cmd_query = self.prev_cmd_query.as_deref().unwrap_or("");

        let (indices, selections) = get_selected_items();
        let tmp: Vec<Cow<str>> = selections.iter().map(|item| item.text()).collect();
        let selected_texts: Vec<&str> = tmp.iter().map(|cow| cow.as_ref()).collect();

        let columns = self.width.load(Ordering::Relaxed);
        let lines = self.height.load(Ordering::Relaxed);

        let inject_context = InjectContext {
            current_index: new_item_index,
            delimiter: &self.delimiter,
            current_selection: &current_selection,
            selections: &selected_texts,
            indices: &indices,
            query,
            cmd_query,
        };

        let preview_context = PreviewContext {
            query,
            cmd_query,
            width: columns,
            height: lines,
            current_index: new_item_index,
            current_selection: &current_selection,
            selected_indices: &indices,
            selections: &selected_texts,
        };

        let preview_event = match new_item {
            Some(item) => match (item.preview(preview_context), PreviewPosition::default()) {
                (ItemPreview::Text(text), pos) => PreviewEvent::PreviewPlainText(text, pos),
                (ItemPreview::AnsiText(text), pos) => PreviewEvent::PreviewAnsiText(text, pos),
                (ItemPreview::TextWithPos(text, pos), _) => PreviewEvent::PreviewPlainText(text, pos),
                (ItemPreview::AnsiWithPos(text, pos), _) => PreviewEvent::PreviewAnsiText(text, pos),
                (ItemPreview::Command(cmd), pos) | (ItemPreview::CommandWithPos(cmd, pos), _) => {
                    if depends_on_items(&cmd) && self.prev_item.is_none() {
                        debug!("the command for preview refers to items and currently there is no item");
                        debug!("command to execute: [{}]", cmd);
                        PreviewEvent::PreviewPlainText("no item matched".to_string(), Default::default())
                    } else {
                        let cmd = inject_command(&cmd, inject_context).to_string();
                        let preview_command = PreviewCommand { cmd, columns, lines };
                        PreviewEvent::PreviewCommand(preview_command, pos)
                    }
                }
                (ItemPreview::Global, _) => {
                    let cmd = self.preview_cmd.clone().expect("previewer: not provided");
                    if depends_on_items(&cmd) && self.prev_item.is_none() {
                        debug!("the command for preview refers to items and currently there is no item");
                        debug!("command to execute: [{}]", cmd);
                        PreviewEvent::PreviewPlainText("no item matched".to_string(), Default::default())
                    } else {
                        let cmd = inject_command(&cmd, inject_context).to_string();
                        let pos = self.eval_scroll_offset(inject_context);
                        let preview_command = PreviewCommand { cmd, columns, lines };
                        PreviewEvent::PreviewCommand(preview_command, pos)
                    }
                }
            },
            None => PreviewEvent::Noop,
        };

        let _ = self.tx_preview.send(preview_event);
    }

    fn act_scroll_down(&mut self, diff: i32) {
        let vscroll_offset = self.vscroll_offset.load(Ordering::SeqCst);
        let new_offset = if diff > 0 {
            vscroll_offset + diff as usize
        } else {
            vscroll_offset - min((-diff) as usize, vscroll_offset)
        };

        let new_offset = min(new_offset, max(self.content_lines.lock().len(), 1) - 1);
        self.vscroll_offset.store(max(new_offset, 1), Ordering::SeqCst);
    }

    fn act_scroll_right(&mut self, diff: i32) {
        let hscroll_offset = self.hscroll_offset.load(Ordering::SeqCst);
        let new_offset = if diff > 0 {
            hscroll_offset + diff as usize
        } else {
            hscroll_offset - min((-diff) as usize, hscroll_offset)
        };
        self.hscroll_offset.store(max(1, new_offset), Ordering::SeqCst);
    }

    fn act_toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
    }

    fn eval_scroll_offset(&self, context: InjectContext) -> PreviewPosition {
        // currently, only h_scroll and h_offset is supported
        // The syntax follows fzf's

        // +SCROLL[-OFFSET] determines the initial scroll offset of the preview window.
        // SCROLL can be either a numeric integer or a  single-field index expression
        // that refers to a numeric integer. The optional -OFFSET part is for adjusting
        // the base offset so that you can see the text above it. It should be given as a
        // numeric integer (-INTEGER), or as a  denominator form (-/INTEGER) for
        // specifying a fraction of the preview window height

        if self.preview_offset.is_empty() {
            return Default::default();
        }

        let offset_expr = inject_command(&self.preview_offset, context);
        if offset_expr.is_empty() {
            return Default::default();
        }

        let nums: Vec<&str> = offset_expr.split('-').collect();
        let v_scroll = if nums.is_empty() {
            Size::Default
        } else {
            Size::Fixed(atoi::<usize>(nums[0]).unwrap_or(0))
        };

        let v_offset = if nums.len() >= 2 {
            let expr = nums[1];
            if expr.starts_with('/') {
                let num = atoi::<usize>(expr).unwrap_or(0);
                Size::Percent(if num == 0 { 0 } else { 100 / num })
            } else {
                let num = atoi::<usize>(expr).unwrap_or(0);
                Size::Fixed(num)
            }
        } else {
            Size::Default
        };

        PreviewPosition {
            h_scroll: Default::default(),
            h_offset: Default::default(),
            v_scroll,
            v_offset,
        }
    }
}

impl Drop for Previewer {
    fn drop(&mut self) {
        let _ = self.tx_preview.send(PreviewEvent::Abort);
        self.thread_previewer.take().map(|handle| handle.join());
    }
}

impl EventHandler for Previewer {
    fn handle(&mut self, event: &Event) -> UpdateScreen {
        use crate::event::Event::*;
        let height = self.height.load(Ordering::Relaxed);
        match event {
            EvActTogglePreviewWrap => self.act_toggle_wrap(),
            EvActPreviewUp(diff) => self.act_scroll_down(-*diff),
            EvActPreviewDown(diff) => self.act_scroll_down(*diff),
            EvActPreviewLeft(diff) => self.act_scroll_right(-*diff),
            EvActPreviewRight(diff) => self.act_scroll_right(*diff),
            EvActPreviewPageUp(diff) => self.act_scroll_down(-(height as i32 * *diff)),
            EvActPreviewPageDown(diff) => self.act_scroll_down(height as i32 * *diff),
            _ => return UpdateScreen::DONT_REDRAW,
        }
        UpdateScreen::REDRAW
    }
}

impl Draw for Previewer {
    fn draw(&self, canvas: &mut dyn Canvas) -> DrawResult<()> {
        canvas.clear()?;
        let (screen_width, screen_height) = canvas.size()?;
        clear_canvas(canvas)?;

        if screen_width == 0 || screen_height == 0 {
            return Ok(());
        }

        self.width.store(screen_width, Ordering::Relaxed);
        self.height.store(screen_height, Ordering::Relaxed);

        let content = self.content_lines.lock();

        let vscroll_offset = self.vscroll_offset.load(Ordering::SeqCst);
        let hscroll_offset = self.hscroll_offset.load(Ordering::SeqCst);

        let mut printer = PrinterBuilder::default()
            .width(screen_width)
            .height(screen_height)
            .skip_rows(max(1, vscroll_offset) - 1)
            .skip_cols(max(1, hscroll_offset) - 1)
            .wrap(self.wrap)
            .build()
            .unwrap();
        printer.print_lines(canvas, &content);

        // print the vscroll info
        let status = format!("{}/{}", vscroll_offset, content.len());
        let col = max(status.len() + 1, screen_width - status.len() - 1);
        canvas.print_with_attr(
            0,
            col,
            &status,
            Attr {
                effect: Effect::REVERSE,
                ..Attr::default()
            },
        )?;

        Ok(())
    }
}

impl Widget<Event> for Previewer {
    fn on_event(&self, event: TermEvent, _rect: Rectangle) -> Vec<Event> {
        let mut ret = vec![];
        match event {
            TermEvent::Key(Key::WheelUp(.., count)) => ret.push(Event::EvActPreviewUp(count as i32)),
            TermEvent::Key(Key::WheelDown(.., count)) => ret.push(Event::EvActPreviewDown(count as i32)),
            _ => {}
        }
        ret
    }
}

#[derive(Debug, Ord, PartialOrd, PartialEq, Eq)]
pub struct PreviewCommand {
    pub cmd: String,
    pub lines: usize,
    pub columns: usize,
}

#[derive(Debug)]
enum PreviewEvent {
    PreviewCommand(PreviewCommand, PreviewPosition),
    PreviewPlainText(String, PreviewPosition),
    PreviewAnsiText(String, PreviewPosition),
    Noop,
    Abort,
}

struct PreviewThread {
    pid: u32,
    thread: thread::JoinHandle<()>,
    stopped: Arc<AtomicBool>,
}

impl PreviewThread {
    fn kill(self) {
        if !self.stopped.load(Ordering::Relaxed) {
            unsafe { libc::kill(self.pid as i32, libc::SIGKILL) };
        }
        self.thread.join().expect("Failed to join Preview process");
    }
}

fn run<C>(rx_preview: Receiver<PreviewEvent>, on_return: C)
where
    C: Fn(Vec<AnsiString<'static>>, PreviewPosition) + Send + Sync + 'static,
{
    let callback = Arc::new(on_return);
    let mut preview_thread: Option<PreviewThread> = None;
    while let Ok(_event) = rx_preview.recv() {
        if preview_thread.is_some() {
            preview_thread.unwrap().kill();
            preview_thread = None;
        }

        let mut event = match _event {
            PreviewEvent::Abort => return,
            _ => _event,
        };

        // Try to empty the channel. Happens when spamming up/down or typing fast.
        while let Ok(_event) = rx_preview.try_recv() {
            event = match _event {
                PreviewEvent::Abort => return,
                _ => _event,
            }
        }

        match event {
            PreviewEvent::PreviewCommand(preview_cmd, pos) => {
                let cmd = &preview_cmd.cmd;
                if cmd.is_empty() {
                    continue;
                }

                let shell = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
                let spawned = Command::new(shell)
                    .env("LINES", preview_cmd.lines.to_string())
                    .env("COLUMNS", preview_cmd.columns.to_string())
                    .arg("-c")
                    .arg(&cmd)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn();

                match spawned {
                    Err(err) => {
                        let astdout = AnsiString::parse(format!("Failed to spawn: {} / {}", cmd, err).as_str());
                        callback(vec![astdout], pos);
                        preview_thread = None;
                    }
                    Ok(spawned) => {
                        let pid = spawned.id();
                        let stopped = Arc::new(AtomicBool::new(false));
                        let stopped_clone = stopped.clone();
                        let callback_clone = callback.clone();
                        let thread = thread::spawn(move || {
                            wait(spawned, move |lines| {
                                stopped_clone.store(true, Ordering::SeqCst);
                                callback_clone(lines, pos);
                            })
                        });
                        preview_thread = Some(PreviewThread { pid, thread, stopped });
                    }
                }
            }
            PreviewEvent::PreviewPlainText(text, pos) => {
                callback(text.lines().map(|line| line.to_string().into()).collect(), pos);
            }
            PreviewEvent::PreviewAnsiText(text, pos) => {
                let mut parser = ANSIParser::default();
                let color_lines = text.lines().map(|line| parser.parse_ansi(line)).collect();
                callback(color_lines, pos);
            }
            PreviewEvent::Noop => {}
            PreviewEvent::Abort => return,
        };
    }
}

fn wait<C>(spawned: std::process::Child, callback: C)
where
    C: Fn(Vec<AnsiString<'static>>),
{
    let output = spawned.wait_with_output();

    if output.is_err() {
        return;
    }

    let output = output.unwrap();

    if output.status.code().is_none() {
        // On Unix it means the process is terminated by a signal
        // directly return to avoid flickering
        return;
    }

    // Capture stderr in case users want to debug ...
    let out_str = String::from_utf8_lossy(if output.status.success() {
        &output.stdout
    } else {
        &output.stderr
    });

    let lines = out_str.lines().map(AnsiString::parse).collect();
    callback(lines);
}

#[derive(Builder, Default, Debug)]
#[builder(default)]
struct Printer {
    #[builder(setter(skip))]
    row: usize,
    #[builder(setter(skip))]
    col: usize,
    skip_rows: usize,
    skip_cols: usize,
    wrap: bool,
    width: usize,
    height: usize,
}

impl Printer {
    pub fn print_lines(&mut self, canvas: &mut dyn Canvas, content: &[AnsiString]) {
        for (line_no, line) in content.iter().enumerate() {
            if line_no < self.skip_rows {
                self.move_to_next_line();
                continue;
            } else if self.row >= self.skip_rows + self.height {
                break;
            }

            for (ch, attr) in line.iter() {
                let _ = self.print_char_with_attr(canvas, ch, attr);

                // skip if the content already exceeded the canvas
                if !self.wrap && self.col >= self.width + self.skip_cols {
                    break;
                }

                if self.row >= self.skip_rows + self.height {
                    break;
                }
            }

            self.move_to_next_line();
        }
    }

    fn move_to_next_line(&mut self) {
        self.row += 1;
        self.col = 0;
    }

    fn print_char_with_attr(&mut self, canvas: &mut dyn Canvas, ch: char, attr: Attr) -> Result<()> {
        match ch {
            '\n' | '\r' | '\0' => {}
            '\t' => {
                // handle tabstop
                let rest = TAB_STOP - self.col % TAB_STOP;
                let rest = min(rest, max(self.col, self.width) - self.col);
                for _ in 0..rest {
                    self.print_char_raw(canvas, ' ', attr)?;
                }
            }

            ch => {
                self.print_char_raw(canvas, ch, attr)?;
            }
        }
        Ok(())
    }

    fn print_char_raw(&mut self, canvas: &mut dyn Canvas, ch: char, attr: Attr) -> Result<()> {
        if self.row < self.skip_rows || self.row >= self.height + self.skip_rows {
            return Ok(());
        }

        if self.wrap {
            // if wrap is enabled, hscroll is discarded
            self.col += self.adjust_scroll_print(canvas, ch, attr)?;

            if self.col >= self.width {
                // re-print the wide character
                self.move_to_next_line();
            }

            if self.col > self.width {
                self.col += self.adjust_scroll_print(canvas, ch, attr)?;
            }
        } else {
            self.col += self.adjust_scroll_print(canvas, ch, attr)?;
        }

        Ok(())
    }

    fn adjust_scroll_print(&self, canvas: &mut dyn Canvas, ch: char, attr: Attr) -> Result<usize> {
        if self.row < self.skip_rows || self.col < self.skip_cols {
            canvas.put_char_with_attr(usize::max_value(), usize::max_value(), ch, attr)
        } else {
            canvas.put_char_with_attr(self.row - self.skip_rows, self.col - self.skip_cols, ch, attr)
        }
    }
}

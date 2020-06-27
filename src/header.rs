///! header of the items
use crate::ansi::AnsiString;
use crate::event::UpdateScreen;
use crate::event::{Event, EventHandler};
use crate::item::ItemPool;
use crate::theme::ColorTheme;
use crate::theme::DEFAULT_THEME;
use crate::util::{print_item, LinePrinter};
use crate::SkimOptions;
use std::cmp::max;
use std::sync::Arc;
use tuikit::prelude::*;

pub struct Header {
    header: AnsiString<'static>,
    tabstop: usize,
    hscroll_offset: usize,
    reverse: bool,
    theme: Arc<ColorTheme>,

    // for reserved header items
    item_pool: Arc<ItemPool>,
}

impl Header {
    pub fn empty() -> Self {
        Self {
            header: AnsiString::new_empty(),
            tabstop: 8,
            hscroll_offset: 0,
            reverse: false,
            theme: Arc::new(*DEFAULT_THEME),
            item_pool: Arc::new(ItemPool::new()),
        }
    }

    pub fn item_pool(mut self, item_pool: Arc<ItemPool>) -> Self {
        self.item_pool = item_pool;
        self
    }

    pub fn theme(mut self, theme: Arc<ColorTheme>) -> Self {
        self.theme = theme;
        self
    }

    pub fn with_options(mut self, options: &SkimOptions) -> Self {
        if let Some(tabstop_str) = options.tabstop {
            let tabstop = tabstop_str.parse::<usize>().unwrap_or(8);
            self.tabstop = max(1, tabstop);
        }

        if options.layout.starts_with("reverse") {
            self.reverse = true;
        }

        match options.header {
            None => {}
            Some("") => {}
            Some(header) => {
                self.header = AnsiString::parse(header);
            }
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.header.is_empty()
    }

    pub fn act_scroll(&mut self, offset: i32) {
        let mut hscroll_offset = self.hscroll_offset as i32;
        hscroll_offset += offset;
        hscroll_offset = max(0, hscroll_offset);
        self.hscroll_offset = hscroll_offset as usize;
    }

    fn lines_of_header(&self) -> usize {
        let fixed = if self.header.is_empty() { 0 } else { 1 };
        fixed + self.item_pool.reserved().len()
    }
}

impl Draw for Header {
    fn draw(&self, canvas: &mut dyn Canvas) -> Result<()> {
        let (screen_width, screen_height) = canvas.size()?;
        if screen_width < 3 {
            return Err("screen width is too small".into());
        }

        if screen_height < self.lines_of_header() {
            return Err("screen height is too small".into());
        }

        canvas.clear()?;

        if !self.is_empty() {
            // print fixed header(specified by --header)
            let mut printer = LinePrinter::builder()
                .row(if self.reverse { 0 } else { screen_height - 1 })
                .col(2)
                .tabstop(self.tabstop)
                .container_width(screen_width - 2)
                .shift(0)
                .text_width(screen_width - 2)
                .hscroll_offset(self.hscroll_offset)
                .build();

            for (ch, _attr) in self.header.iter() {
                printer.print_char(canvas, ch, self.theme.header(), false);
            }
        }

        let lines_used = if !self.is_empty() { 1 } else { 0 };

        // print "reserved" header lines (--header-lines)
        for (idx, item) in self.item_pool.reserved().iter().enumerate() {
            let row = if self.reverse {
                idx + lines_used
            } else {
                screen_height - lines_used - idx - 1
            };

            let mut printer = LinePrinter::builder()
                .row(row)
                .col(2)
                .tabstop(self.tabstop)
                .container_width(screen_width - 2)
                .shift(0)
                .text_width(screen_width - 2)
                .hscroll_offset(self.hscroll_offset)
                .build();
            print_item(canvas, &mut printer, item, self.theme.header());
        }

        Ok(())
    }
}

impl Widget<Event> for Header {
    fn size_hint(&self) -> (Option<usize>, Option<usize>) {
        (None, Some(self.lines_of_header()))
    }
}

impl EventHandler for Header {
    fn handle(&mut self, event: &Event) -> UpdateScreen {
        match event {
            Event::EvActScrollLeft(diff) => {
                self.act_scroll(*diff);
            }

            Event::EvActScrollRight(diff) => {
                self.act_scroll(*diff);
            }

            _ => {
                return UpdateScreen::DONT_REDRAW;
            }
        }

        UpdateScreen::REDRAW
    }
}

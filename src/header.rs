///! header of the items
use crate::ansi::{ANSIParser, AnsiString};
use crate::event::UpdateScreen;
use crate::event::{Event, EventHandler};
use crate::item::ItemPool;
use crate::theme::ColorTheme;
use crate::theme::DEFAULT_THEME;
use crate::util::{print_item, str_lines, LinePrinter};
use crate::{DisplayContext, Matches, SkimOptions};
use defer_drop::DeferDrop;
use std::cmp::max;
use std::sync::Arc;
use tuikit::prelude::*;

pub struct Header {
    header: Vec<AnsiString<'static>>,
    tabstop: usize,
    hscroll_offset: usize,
    reverse: bool,
    theme: Arc<ColorTheme>,

    // for reserved header items
    item_pool: Arc<DeferDrop<ItemPool>>,
}

impl Header {
    pub fn empty() -> Self {
        Self {
            header: vec![],
            tabstop: 8,
            hscroll_offset: 0,
            reverse: false,
            theme: Arc::new(*DEFAULT_THEME),
            item_pool: Arc::new(DeferDrop::new(ItemPool::new())),
        }
    }

    pub fn item_pool(mut self, item_pool: Arc<DeferDrop<ItemPool>>) -> Self {
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
                let mut parser = ANSIParser::default();
                self.header = str_lines(header).into_iter().map(|l| parser.parse_ansi(l)).collect();
            }
        }
        self
    }

    pub fn act_scroll(&mut self, offset: i32) {
        let mut hscroll_offset = self.hscroll_offset as i32;
        hscroll_offset += offset;
        hscroll_offset = max(0, hscroll_offset);
        self.hscroll_offset = hscroll_offset as usize;
    }

    fn lines_of_header(&self) -> usize {
        self.header.len() + self.item_pool.reserved().len()
    }

    fn adjust_row(&self, index: usize, screen_height: usize) -> usize {
        if self.reverse {
            index
        } else {
            screen_height - index - 1
        }
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

        for (idx, header) in self.header.iter().enumerate() {
            // print fixed header(specified by --header)
            let mut printer = LinePrinter::builder()
                .row(self.adjust_row(idx, screen_height))
                .col(2)
                .tabstop(self.tabstop)
                .container_width(screen_width - 2)
                .shift(0)
                .text_width(screen_width - 2)
                .hscroll_offset(self.hscroll_offset)
                .build();

            for (ch, _attr) in header.iter() {
                printer.print_char(canvas, ch, self.theme.header(), false);
            }
        }

        let lines_used = self.header.len();

        // print "reserved" header lines (--header-lines)
        for (idx, item) in self.item_pool.reserved().iter().enumerate() {
            let mut printer = LinePrinter::builder()
                .row(self.adjust_row(idx + lines_used, screen_height))
                .col(2)
                .tabstop(self.tabstop)
                .container_width(screen_width - 2)
                .shift(0)
                .text_width(screen_width - 2)
                .hscroll_offset(self.hscroll_offset)
                .build();

            let context = DisplayContext {
                text: &item.text(),
                score: 0,
                matches: Matches::None,
                container_width: screen_width - 2,
                highlight_attr: self.theme.header(),
            };

            print_item(canvas, &mut printer, item, context, self.theme.header());
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

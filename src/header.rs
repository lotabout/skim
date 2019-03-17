///! header of the items
use crate::ansi::AnsiString;
use crate::event::UpdateScreen;
use crate::event::{Event, EventArg, EventHandler};
use crate::theme::ColorTheme;
use crate::theme::DEFAULT_THEME;
use crate::util::LinePrinter;
use crate::SkimOptions;
use std::cmp::max;
use std::sync::Arc;
use tuikit::prelude::*;

pub struct Header {
    header: AnsiString,
    tabstop: usize,
    hscroll_offset: usize,
    theme: Arc<ColorTheme>,
}

#[allow(dead_code)]
impl Header {
    pub fn new(header: AnsiString) -> Self {
        Self {
            header,
            tabstop: 8,
            hscroll_offset: 0,
            theme: Arc::new(DEFAULT_THEME),
        }
    }

    pub fn empty() -> Self {
        Self::new(AnsiString::new_empty())
    }

    pub fn with_options(options: &SkimOptions) -> Self {
        let mut ret = Self::empty();
        ret.parse_options(options);
        ret
    }

    pub fn is_empty(&self) -> bool {
        self.header.is_empty()
    }

    fn parse_options(&mut self, options: &SkimOptions) {
        if let Some(tabstop_str) = options.tabstop {
            let tabstop = tabstop_str.parse::<usize>().unwrap_or(8);
            self.tabstop = max(1, tabstop);
        }

        match options.header {
            None => {}
            Some("") => {}
            Some(header) => {
                self.header = AnsiString::from_str(header);
            }
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

    pub fn act_scroll(&mut self, offset: i32) {
        let mut hscroll_offset = self.hscroll_offset as i32;
        hscroll_offset += offset;
        hscroll_offset = max(0, hscroll_offset);
        self.hscroll_offset = hscroll_offset as usize;
    }
}

impl Draw for Header {
    fn draw(&self, canvas: &mut Canvas) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }

        let (screen_width, _screen_height) = canvas.size()?;
        if screen_width < 3 {
            return Err("screen width is too small".into());
        }

        canvas.clear()?;

        let mut printer = LinePrinter::builder()
            .col(2)
            .tabstop(self.tabstop)
            .container_width(screen_width - 2)
            .shift(0)
            .text_width(screen_width - 2)
            .hscroll_offset(self.hscroll_offset)
            .build();

        for (ch, attr) in self.header.iter() {
            printer.print_char(canvas, ch, self.theme.normal().extend(attr), false);
        }

        Ok(())
    }
}

impl EventHandler for Header {
    fn accept_event(&self, event: Event) -> bool {
        event == Event::EvActScrollLeft || event == Event::EvActScrollRight
    }

    fn handle(&mut self, event: Event, arg: &EventArg) -> UpdateScreen {
        match event {
            Event::EvActScrollLeft => {
                self.act_scroll(*arg.downcast_ref::<i32>().unwrap_or(&-1));
            }

            Event::EvActScrollRight => {
                self.act_scroll(*arg.downcast_ref::<i32>().unwrap_or(&1));
            }

            _ => {
                return UpdateScreen::DontRedraw;
            }
        }

        UpdateScreen::Redraw
    }
}

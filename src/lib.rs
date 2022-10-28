#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;

use std::any::Any;
use std::borrow::Cow;
use std::fmt::Display;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;

use crossbeam::channel::{Receiver, Sender};
use tuikit::prelude::{Event as TermEvent, *};

pub use crate::ansi::AnsiString;
pub use crate::engine::fuzzy::FuzzyAlgorithm;
use crate::event::{EventReceiver, EventSender};
use crate::model::Model;
pub use crate::options::SkimOptions;
pub use crate::output::SkimOutput;
use crate::reader::Reader;

mod ansi;
mod engine;
mod event;
pub mod field;
mod global;
mod header;
mod helper;
mod input;
mod item;
mod matcher;
mod model;
mod options;
mod orderedvec;
mod output;
pub mod prelude;
mod previewer;
mod query;
mod reader;
mod selection;
mod spinlock;
mod theme;
mod util;

//------------------------------------------------------------------------------
pub trait AsAny {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// A `SkimItem` defines what's been processed(fetched, matched, previewed and returned) by skim
///
/// # Downcast Example
/// Skim will return the item back, but in `Arc<dyn SkimItem>` form. We might want a reference
/// to the concrete type instead of trait object. Skim provide a somehow "complicated" way to
/// `downcast` it back to the reference of the original concrete type.
///
/// ```rust
/// use skim::prelude::*;
///
/// struct MyItem {}
/// impl SkimItem for MyItem {
///     fn text(&self) -> Cow<str> {
///         unimplemented!()
///     }
/// }
///
/// impl MyItem {
///     pub fn mutable(&mut self) -> i32 {
///         1
///     }
///
///     pub fn immutable(&self) -> i32 {
///         0
///     }
/// }
///
/// let mut ret: Arc<dyn SkimItem> = Arc::new(MyItem{});
/// let mutable: &mut MyItem = Arc::get_mut(&mut ret)
///     .expect("item is referenced by others")
///     .as_any_mut() // cast to Any
///     .downcast_mut::<MyItem>() // downcast to (mut) concrete type
///     .expect("something wrong with downcast");
/// assert_eq!(mutable.mutable(), 1);
///
/// let immutable: &MyItem = (*ret).as_any() // cast to Any
///     .downcast_ref::<MyItem>() // downcast to concrete type
///     .expect("something wrong with downcast");
/// assert_eq!(immutable.immutable(), 0)
/// ```
pub trait SkimItem: AsAny + Send + Sync + 'static {
    /// The string to be used for matching (without color)
    fn text(&self) -> Cow<str>;

    /// The content to be displayed on the item list, could contain ANSI properties
    fn display<'a>(&'a self, context: DisplayContext<'a>) -> AnsiString<'a> {
        AnsiString::from(context)
    }

    /// Custom preview content, default to `ItemPreview::Global` which will use global preview
    /// setting(i.e. the command set by `preview` option)
    fn preview(&self, _context: PreviewContext) -> ItemPreview {
        ItemPreview::Global
    }

    /// Get output text(after accept), default to `text()`
    /// Note that this function is intended to be used by the caller of skim and will not be used by
    /// skim. And since skim will return the item back in `SkimOutput`, if string is not what you
    /// want, you could still use `downcast` to retain the pointer to the original struct.
    fn output(&self) -> Cow<str> {
        self.text()
    }

    /// we could limit the matching ranges of the `get_text` of the item.
    /// providing (start_byte, end_byte) of the range
    fn get_matching_ranges(&self) -> Option<&[(usize, usize)]> {
        None
    }
}

//------------------------------------------------------------------------------
// Implement SkimItem for raw strings

impl<T: AsRef<str> + Send + Sync + 'static> SkimItem for T {
    fn text(&self) -> Cow<str> {
        Cow::Borrowed(self.as_ref())
    }
}

//------------------------------------------------------------------------------
// Display Context
pub enum Matches<'a> {
    None,
    CharIndices(&'a [usize]),
    CharRange(usize, usize),
    ByteRange(usize, usize),
}

pub struct DisplayContext<'a> {
    pub text: &'a str,
    pub score: i32,
    pub matches: Matches<'a>,
    pub container_width: usize,
    pub highlight_attr: Attr,
}

impl<'a> From<DisplayContext<'a>> for AnsiString<'a> {
    fn from(context: DisplayContext<'a>) -> Self {
        match context.matches {
            Matches::CharIndices(indices) => AnsiString::from((context.text, indices, context.highlight_attr)),
            Matches::CharRange(start, end) => {
                AnsiString::new_str(context.text, vec![(context.highlight_attr, (start as u32, end as u32))])
            }
            Matches::ByteRange(start, end) => {
                let ch_start = context.text[..start].chars().count();
                let ch_end = ch_start + context.text[start..end].chars().count();
                AnsiString::new_str(
                    context.text,
                    vec![(context.highlight_attr, (ch_start as u32, ch_end as u32))],
                )
            }
            Matches::None => AnsiString::new_str(context.text, vec![]),
        }
    }
}

//------------------------------------------------------------------------------
// Preview Context

pub struct PreviewContext<'a> {
    pub query: &'a str,
    pub cmd_query: &'a str,
    pub width: usize,
    pub height: usize,
    pub current_index: usize,
    pub current_selection: &'a str,
    /// selected item indices (may or may not include current item)
    pub selected_indices: &'a [usize],
    /// selected item texts (may or may not include current item)
    pub selections: &'a [&'a str],
}

//------------------------------------------------------------------------------
// Preview
#[derive(Default, Copy, Clone, Debug)]
pub struct PreviewPosition {
    pub h_scroll: Size,
    pub h_offset: Size,
    pub v_scroll: Size,
    pub v_offset: Size,
}

pub enum ItemPreview {
    /// execute the command and print the command's output
    Command(String),
    /// Display the prepared text(lines)
    Text(String),
    /// Display the colored text(lines)
    AnsiText(String),
    CommandWithPos(String, PreviewPosition),
    TextWithPos(String, PreviewPosition),
    AnsiWithPos(String, PreviewPosition),
    /// Use global command settings to preview the item
    Global,
}

//==============================================================================
// A match engine will execute the matching algorithm

#[derive(Eq, PartialEq, Debug, Copy, Clone)]
pub enum CaseMatching {
    Respect,
    Ignore,
    Smart,
}

impl Default for CaseMatching {
    fn default() -> Self {
        CaseMatching::Smart
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
#[allow(dead_code)]
pub enum MatchRange {
    ByteRange(usize, usize),
    // range of bytes
    Chars(Vec<usize>), // individual character indices matched
}

pub type Rank = [i32; 4];

#[derive(Clone)]
pub struct MatchResult {
    pub rank: Rank,
    pub matched_range: MatchRange,
}

impl MatchResult {
    pub fn range_char_indices(&self, text: &str) -> Vec<usize> {
        match &self.matched_range {
            &MatchRange::ByteRange(start, end) => {
                let first = text[..start].chars().count();
                let last = first + text[start..end].chars().count();
                (first..last).collect()
            }
            MatchRange::Chars(vec) => vec.clone(),
        }
    }
}

pub trait MatchEngine: Sync + Send + Display {
    fn match_item(&self, item: Arc<dyn SkimItem>) -> Option<MatchResult>;
}

pub trait MatchEngineFactory {
    fn create_engine_with_case(&self, query: &str, case: CaseMatching) -> Box<dyn MatchEngine>;
    fn create_engine(&self, query: &str) -> Box<dyn MatchEngine> {
        self.create_engine_with_case(query, CaseMatching::default())
    }
}

//------------------------------------------------------------------------------
// Preselection

/// A selector that determines whether an item should be "pre-selected" in multi-selection mode
pub trait Selector {
    fn should_select(&self, index: usize, item: &dyn SkimItem) -> bool;
}

//------------------------------------------------------------------------------
pub type SkimItemSender = Sender<Arc<dyn SkimItem>>;
pub type SkimItemReceiver = Receiver<Arc<dyn SkimItem>>;

pub struct Skim {}

impl Skim {
    /// params:
    /// - options: the "complex" options that control how skim behaves
    /// - source: a stream of items to be passed to skim for filtering.
    ///   If None is given, skim will invoke the command given to fetch the items.
    ///
    /// return:
    /// - None: on internal errors.
    /// - SkimOutput: the collected key, event, query, selected items, etc.
    pub fn run_with(options: &SkimOptions, source: Option<SkimItemReceiver>) -> Option<SkimOutput> {
        let min_height = options
            .min_height
            .map(Skim::parse_height_string)
            .expect("min_height should have default values");
        let height = options
            .height
            .map(Skim::parse_height_string)
            .expect("height should have default values");

        let (tx, rx): (EventSender, EventReceiver) = channel();
        let term = Arc::new(
            Term::with_options(
                TermOptions::default()
                    .min_height(min_height)
                    .height(height)
                    .clear_on_exit(!options.no_clear)
                    .disable_alternate_screen(options.no_clear_start)
                    .clear_on_start(!options.no_clear_start)
                    .hold(options.select1 || options.exit0 || options.sync),
            )
            .unwrap(),
        );
        if !options.no_mouse {
            let _ = term.enable_mouse_support();
        }

        //------------------------------------------------------------------------------
        // input
        let mut input = input::Input::new();
        input.parse_keymaps(&options.bind);
        input.parse_expect_keys(options.expect.as_deref());

        let tx_clone = tx.clone();
        let term_clone = term.clone();
        let input_thread = thread::spawn(move || loop {
            if let Ok(key) = term_clone.poll_event() {
                if key == TermEvent::User(()) {
                    break;
                }

                let (key, action_chain) = input.translate_event(key);
                for event in action_chain.into_iter() {
                    let _ = tx_clone.send((key, event));
                }
            }
        });

        //------------------------------------------------------------------------------
        // reader

        let reader = Reader::with_options(options).source(source);

        //------------------------------------------------------------------------------
        // model + previewer
        let mut model = Model::new(rx, tx, reader, term.clone(), options);
        let ret = model.start();
        let _ = term.send_event(TermEvent::User(())); // interrupt the input thread
        let _ = input_thread.join();
        ret
    }

    // 10 -> TermHeight::Fixed(10)
    // 10% -> TermHeight::Percent(10)
    fn parse_height_string(string: &str) -> TermHeight {
        if string.ends_with('%') {
            TermHeight::Percent(string[0..string.len() - 1].parse().unwrap_or(100))
        } else {
            TermHeight::Fixed(string.parse().unwrap_or(0))
        }
    }
}

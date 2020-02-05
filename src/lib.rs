#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
mod ansi;
mod engine;
mod event;
mod field;
mod header;
mod input;
mod item;
mod item_collector;
mod matcher;
mod model;
mod options;
mod orderedvec;
mod output;
pub mod prelude;
mod previewer;
mod query;
mod reader;
mod score;
mod selection;
mod spinlock;
mod theme;
mod util;

pub use crate::ansi::AnsiString;
use crate::event::{EventReceiver, EventSender};
pub use crate::item_collector::*;
use crate::model::Model;
pub use crate::options::{SkimOptions, SkimOptionsBuilder};
pub use crate::output::SkimOutput;
use crate::reader::Reader;
pub use crate::score::FuzzyAlgorithm;
use crossbeam::channel::{Receiver, Sender};
use std::borrow::Cow;
use std::env;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use tuikit::prelude::{Event as TermEvent, *};

//------------------------------------------------------------------------------
pub trait SkimItem: Send + Sync {
    /// The text to be displayed on the item list, could contain ANSI properties
    fn display(&self) -> Cow<AnsiString>;

    /// helper function to get pure text presentation(without color) of the item
    fn get_text(&self) -> Cow<str>;

    /// get output text(after accept), could be override
    fn output(&self) -> Cow<str> {
        self.get_text()
    }

    fn preview(&self) -> ItemPreview {
        ItemPreview::Global
    }

    /// we could limit the matching ranges of the `get_text` of the item.
    /// providing (start_byte, end_byte) of the range
    fn get_matching_ranges(&self) -> Cow<[(usize, usize)]> {
        Cow::Owned(vec![(0, self.display().stripped().len())])
    }
}

impl<T: AsRef<str> + Send + Sync> SkimItem for T {
    fn display(&self) -> Cow<AnsiString> {
        Cow::Owned(AnsiString::new_str(self.as_ref()))
    }

    fn get_text(&self) -> Cow<str> {
        Cow::Borrowed(self.as_ref())
    }
}

//------------------------------------------------------------------------------
// Preview
pub enum ItemPreview {
    Command(String),
    Text(String),
    Global,
}

//------------------------------------------------------------------------------
pub type SkimItemSender = Sender<Arc<dyn SkimItem>>;
pub type SkimItemReceiver = Receiver<Arc<dyn SkimItem>>;

pub struct Skim {}
impl Skim {
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
        let term = Arc::new(Term::with_options(TermOptions::default().min_height(min_height).height(height)).unwrap());
        if !options.no_mouse {
            let _ = term.enable_mouse_support();
        }

        //------------------------------------------------------------------------------
        // input
        let mut input = input::Input::new();
        input.parse_keymaps(&options.bind);
        input.parse_expect_keys(options.expect.as_ref().map(|x| &**x));
        let tx_clone = tx.clone();
        let term_clone = term.clone();
        let input_thread = thread::spawn(move || loop {
            if let Ok(key) = term_clone.poll_event() {
                if key == TermEvent::User1 {
                    break;
                }

                for ev in input.translate_event(key).into_iter() {
                    let _ = tx_clone.send(ev);
                }
            }
        });

        //------------------------------------------------------------------------------
        // reader

        let reader = Reader::with_options(&options).source(source);

        //------------------------------------------------------------------------------
        // model + previewer
        let mut model = Model::new(rx, tx, reader, term.clone(), &options);
        let ret = model.start();
        let _ = term.send_event(TermEvent::User1); // interrupt the input thread
        let _ = input_thread.join();
        let _ = term.pause();
        ret
    }

    pub fn filter(options: &SkimOptions, source: Option<SkimItemReceiver>) -> i32 {
        use crate::engine::{EngineFactory, MatcherMode};

        let output_ending = if options.print0 { "\0" } else { "\n" };
        let query = options.filter;
        let default_command = match env::var("SKIM_DEFAULT_COMMAND").as_ref().map(String::as_ref) {
            Ok("") | Err(_) => "find .".to_owned(),
            Ok(val) => val.to_owned(),
        };

        let cmd = options.cmd.unwrap_or(&default_command);

        // output query
        if options.print_query {
            print!("{}{}", query, output_ending);
        }

        if options.print_cmd {
            print!("{}{}", cmd, output_ending);
        }

        //------------------------------------------------------------------------------
        // reader

        let mut reader = Reader::with_options(&options).source(source);

        //------------------------------------------------------------------------------
        // matcher
        let matcher_mode = if options.regex {
            MatcherMode::Regex
        } else if options.exact {
            MatcherMode::Exact
        } else {
            MatcherMode::Fuzzy
        };

        let engine = EngineFactory::build(query, matcher_mode, options.algorithm);

        //------------------------------------------------------------------------------
        // start
        let reader_control = reader.run(cmd);

        let mut match_count = 0;
        while !reader_control.is_done() {
            for item in reader_control.take().into_iter() {
                if let Some(matched) = engine.match_item(item) {
                    if options.print_score {
                        println!("{}\t{}", -matched.rank.score, matched.item.output());
                    } else {
                        println!("{}", matched.item.output());
                    }
                    match_count += 1;
                }
            }
        }

        if match_count == 0 {
            1
        } else {
            0
        }
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

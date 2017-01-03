use std::sync::mpsc::{Receiver, Sender};
use event::{Event, EventArg};
use item::{Item, MatchedItem, MatchedRange};

use getopts;
use score;
use std::io::Write;
use regex::Regex;

macro_rules! println_stderr(
    ($($arg:tt)*) => { {
        let r = writeln!(&mut ::std::io::stderr(), $($arg)*);
        r.expect("failed printing to stderr");
    } }
);

#[derive(Clone, Copy)]
enum Algorithm {
    FUZZY,
    REGEX,
    PREFIX_EXACT,
    SUFFIX_EXACT,
    EXACT,
    INVERSE_EXACT,
    INVERSE_SUFFIX_EXACT,
}

pub struct Matcher {
    tx_result: Sender<(Event, EventArg)>,
    rx_item: Receiver<(Event, EventArg)>,
    rank_criterion: Vec<RankCriteria>,
    is_exact: bool,
}

impl Matcher {
    pub fn new(rx_item: Receiver<(Event, EventArg)>, tx_result: Sender<(Event, EventArg)>) -> Self {
        Matcher {
            rx_item: rx_item,
            tx_result: tx_result,
            rank_criterion: vec![RankCriteria::Score, RankCriteria::Index, RankCriteria::Begin, RankCriteria::End],
            is_exact: false,
        }
    }

    pub fn parse_options(&mut self, options: &getopts::Matches) {
        if let Some(tie_breaker) = options.opt_str("t") {
            let mut vec = Vec::new();
            for criteria in tie_breaker.split(',') {
                if let Some(c) = parse_criteria(criteria) {
                    vec.push(c);
                }
            }
            self.rank_criterion = vec;
        }

        if options.opt_present("exact") {
            self.is_exact = true;
        }
    }

    pub fn run(&self) {
        let mut matcher_engine: Option<MatchingEngine> = None;
        let mut total_num: usize = 0;
        while let Ok((ev, arg)) = self.rx_item.recv() {
            match ev {
                Event::EvMatcherNewItem => {
                    total_num += 1;
                    let item = *arg.downcast::<Item>().unwrap();

                    matcher_engine.as_ref().map(|mat| {
                        let matched_item = mat.match_item(item);
                        if matched_item != None {
                            let _ = self.tx_result.send((Event::EvModelNewItem, Box::new(matched_item.unwrap())));
                        }
                    });

                    // report total number
                    if total_num % 11 == 0 {
                        let _ = self.tx_result.send((Event::EvModelNotifyTotal, Box::new(total_num)));
                    }
                }

                Event::EvSenderStopped | Event::EvReaderStopped => {
                    let _ = self.tx_result.send((Event::EvModelNotifyTotal, Box::new(total_num)));
                    let _ = self.tx_result.send((ev, arg));
                }
                Event::EvReaderStarted => { let _ = self.tx_result.send((ev, arg)); }

                Event::EvMatcherRestart => {
                    total_num = 0;
                    let query = *arg.downcast::<String>().unwrap();

                    // notifiy the model that the query had been changed
                    let _ = self.tx_result.send((Event::EvModelRestart, Box::new(true)));

                    matcher_engine = Some(MatchingEngine::builder(&query, self.is_exact)
                                          .rank(&self.rank_criterion)
                                          .build());
                }

                _ => {}
            }
        }
    }

}

type ExactFilter = Box<Fn(&Item, &Option<((usize, usize), (usize, usize))>) -> bool>;

struct MatchingEngine<'a> {
    query: String,
    query_chars: Vec<char>,
    query_lower_chars: Vec<char>,
    query_regex: Option<Regex>,
    rank_criterion: Option<&'a [RankCriteria]>,
    algorithm: Algorithm
}

impl<'a> MatchingEngine<'a> {
    pub fn builder(query: &str, is_exact: bool) -> Self {
        let (algo, query) = if query.starts_with('\'') {
            if is_exact {
                (Algorithm::FUZZY, &query[1..])
            } else {
                (Algorithm::EXACT, &query[1..])
            }
        } else if query.starts_with('^') {
            (Algorithm::PREFIX_EXACT, &query[1..])
        } else if query.starts_with('!') {
            if query.ends_with('$') {
                (Algorithm::INVERSE_SUFFIX_EXACT, &query[1..(query.len()-1)])
            } else {
                (Algorithm::INVERSE_EXACT, &query[1..])
            }
        } else if query.ends_with('$') {
            (Algorithm::SUFFIX_EXACT, &query[..(query.len()-1)])
        } else {
            if is_exact {
                (Algorithm::EXACT, query)
            } else {
                (Algorithm::FUZZY, query)
            }
        };

        MatchingEngine {
            query: query.to_string(),
            query_chars: query.chars().collect(),
            query_lower_chars: query.to_lowercase().chars().collect(),
            query_regex:  Regex::new(query).ok(),
            rank_criterion: None,
            algorithm: algo,
        }
    }

    pub fn rank(mut self, rank: &'a [RankCriteria]) -> Self {
        self.rank_criterion = Some(rank);
        self
    }

    pub fn build(self) -> Self {
        self
    }

    pub fn match_item(&self, item: Item) -> Option<MatchedItem> {
        match self.algorithm {
            Algorithm::FUZZY => self.match_item_fuzzy(item),
            Algorithm::REGEX => self.match_item_regex(item),
            Algorithm::EXACT => {
                self.match_item_exact(item, Box::new(|_, matched_result| {
                    matched_result.is_some()
                }))
            }
            Algorithm::INVERSE_EXACT => {
                self.match_item_exact(item, Box::new(|_, matched_result| {
                    matched_result.is_none()
                }))
            }
            Algorithm::PREFIX_EXACT => {
                self.match_item_exact(item, Box::new(|_, matched_result| {
                    match *matched_result {
                        Some(((start_pos, _), _)) => start_pos == 0,
                        None => false
                    }
                }))
            }
            Algorithm::SUFFIX_EXACT => {
                self.match_item_exact(item, Box::new(|ref item, matched_result| {
                    match *matched_result {
                        Some((_, (_, last_pos))) => last_pos == item.get_lower_chars().len(),
                        None => false
                    }
                }))
            }
            Algorithm::INVERSE_SUFFIX_EXACT => {
                self.match_item_exact(item, Box::new(|ref item, matched_result| {
                    match *matched_result {
                        Some((_, (_, last_pos))) => last_pos != item.get_lower_chars().len(),
                        None => true
                    }
                }))
            }
        }
    }

    fn build_rank(&self, score: i64, index: i64, begin: i64, end: i64) -> [i64; 4] {
        self.rank_criterion.map(|criterion| {
            let mut rank = [0; 4];
            for (idx, criteria) in criterion.iter().enumerate().take(4) {
                rank[idx] = match *criteria {
                    RankCriteria::Score    => score,
                    RankCriteria::Index    => index,
                    RankCriteria::Begin    => begin,
                    RankCriteria::End      => end,
                    RankCriteria::NegScore => -score,
                    RankCriteria::NegIndex => -index,
                    RankCriteria::NegBegin => -begin,
                    RankCriteria::NegEnd   => -end,
                }
            }
            rank
        }).unwrap_or([0; 4])
    }

    fn match_item_regex(&self, item: Item) -> Option<MatchedItem> {
        let mut matched_result = None;
        for &(start, end) in item.get_matching_ranges() {
            if self.query == "" {
                matched_result = Some((0, 0));
                break;
            }

            let source: String = item.get_lower_chars()[start .. end].iter().map(|&c| c).collect();
            matched_result = score::regex_match(&source, &self.query_regex);

            if matched_result == None {
                continue;
            }
        }

        if matched_result == None {
            return None;
        }

        let (begin, end) = matched_result.unwrap();

        let score = end - begin;
        let rank = self.build_rank(-score, item.get_index() as i64, begin, end);

        Some(MatchedItem::builder(item)
             .rank(rank)
             .matched_range(MatchedRange::Range(begin as usize, end as usize))
             .build())
    }

    fn match_item_fuzzy(&self, item: Item) -> Option<MatchedItem> {
        // iterate over all matching fields:
        let mut matched_result = None;
        for &(start, end) in item.get_matching_ranges() {
            let source = &item.get_lower_chars()[start .. end];

            matched_result = score::fuzzy_match(source, &self.query_chars, &self.query_lower_chars);

            if matched_result == None {
                continue;
            }
        }

        if matched_result == None {
            return None;
        }

        let (score, matched_range) = matched_result.unwrap();

        let begin = *matched_range.get(0).unwrap_or(&0) as i64;
        let end = *matched_range.last().unwrap_or(&0) as i64;

        let rank = self.build_rank(-score, item.get_index() as i64, begin, end);

        Some(MatchedItem::builder(item)
             .rank(rank)
             .matched_range(MatchedRange::Chars(matched_range))
             .build())
    }

    fn match_item_exact(&self, item:Item, filter: ExactFilter) -> Option<MatchedItem>{
        let mut matched_result = None;
        for &(start, end) in item.get_matching_ranges() {
            if self.query == "" {
                matched_result = Some(((0, 0), (0, 0)));
                break;
            }

            let chars: Vec<_> = item.get_text().chars().collect();
            let source: String = chars[start .. end].iter().map(|&c| c).collect();
            matched_result = score::exact_match(&source, &self.query);

            if matched_result == None {
                continue;
            }
        }

        if !filter(&item, &matched_result){
            return None;
        }

        let (first, _) = matched_result.unwrap_or(((0,0), (0,0)));

        let (begin, end) = first;
        let score = (end - begin) as i64;
        let rank = self.build_rank(-score, item.get_index() as i64, begin as i64, end as i64);

        Some(MatchedItem::builder(item)
             .rank(rank)
             .matched_range(MatchedRange::Range(begin, end))
             .build())
    }
}

#[derive(Debug)]
pub enum RankCriteria {
    Score,
    Index,
    Begin,
    End,
    NegScore,
    NegIndex,
    NegBegin,
    NegEnd,
}

pub fn parse_criteria(text: &str) -> Option<RankCriteria> {
    match text.to_lowercase().as_ref() {
        "score"  => Some(RankCriteria::Score),
        "index"  => Some(RankCriteria::Index),
        "begin"  => Some(RankCriteria::Begin),
        "end"    => Some(RankCriteria::End),
        "-score" => Some(RankCriteria::NegScore),
        "-index" => Some(RankCriteria::NegIndex),
        "-begin" => Some(RankCriteria::NegBegin),
        "-end"   => Some(RankCriteria::NegEnd),
        _ => None,
    }
}

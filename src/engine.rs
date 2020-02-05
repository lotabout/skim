///! matcher engine
use crate::item::{ItemWrapper, MatchedItem, MatchedRange, Rank};
use crate::score::FuzzyAlgorithm;
use crate::{score, SkimItem};
use regex::Regex;
use std::sync::Arc;

lazy_static! {
    static ref RE_AND: Regex = Regex::new(r"([^ |]+( +\| +[^ |]*)+)|( +)").unwrap();
    static ref RE_OR: Regex = Regex::new(r" +\| +").unwrap();
}

#[derive(Clone, Copy, PartialEq)]
pub enum MatcherMode {
    Regex,
    Fuzzy,
    Exact,
}

// A match engine will execute the matching algorithm
pub trait MatchEngine: Sync + Send {
    fn match_item(&self, item: Arc<ItemWrapper>) -> Option<MatchedItem>;
    fn display(&self) -> String;
}

fn build_rank(score: i64, index: i64, begin: i64, end: i64) -> Rank {
    Rank {
        score,
        index,
        begin,
        end,
    }
}

//------------------------------------------------------------------------------
// Regular Expression engine
#[derive(Debug)]
struct RegexEngine {
    query_regex: Option<Regex>,
}

impl RegexEngine {
    pub fn builder(query: &str) -> Self {
        RegexEngine {
            query_regex: Regex::new(query).ok(),
        }
    }

    pub fn build(self) -> Self {
        self
    }
}

impl MatchEngine for RegexEngine {
    fn match_item(&self, item: Arc<ItemWrapper>) -> Option<MatchedItem> {
        let mut matched_result = None;
        for &(start, end) in item.get_matching_ranges().as_ref() {
            if self.query_regex.is_none() {
                matched_result = Some((0, 0));
                break;
            }

            matched_result = score::regex_match(&item.get_text()[start..end], &self.query_regex)
                .map(|(s, e)| (s + start, e + start));

            if matched_result.is_some() {
                break;
            }
        }

        let (begin, end) = matched_result?;
        let score = (end - begin) as i64;
        let rank = build_rank(-score, item.get_index() as i64, begin as i64, end as i64);

        Some(
            MatchedItem::builder(item)
                .rank(rank)
                .matched_range(MatchedRange::ByteRange(begin, end))
                .build(),
        )
    }

    fn display(&self) -> String {
        format!(
            "(Regex: {})",
            self.query_regex
                .as_ref()
                .map_or("".to_string(), |re| re.as_str().to_string())
        )
    }
}

//------------------------------------------------------------------------------
// Fuzzy engine
#[derive(Debug)]
struct FuzzyEngine {
    query: String,
    algorithm: FuzzyAlgorithm,
}

impl FuzzyEngine {
    pub fn builder(query: &str) -> Self {
        FuzzyEngine {
            query: query.to_string(),
            algorithm: FuzzyAlgorithm::default(),
        }
    }

    pub fn algorithm(mut self, algorithm: FuzzyAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    pub fn build(self) -> Self {
        self
    }
}

impl MatchEngine for FuzzyEngine {
    fn match_item(&self, item: Arc<ItemWrapper>) -> Option<MatchedItem> {
        // iterate over all matching fields:
        let mut matched_result = None;
        for &(start, end) in item.get_matching_ranges().as_ref() {
            matched_result =
                score::fuzzy_match(&item.get_text()[start..end], &self.query, self.algorithm).map(|(s, vec)| {
                    if start != 0 {
                        let start_char = &item.get_text()[..start].chars().count();
                        (s, vec.iter().map(|x| x + start_char).collect())
                    } else {
                        (s, vec)
                    }
                });

            if matched_result.is_some() {
                break;
            }
        }

        if matched_result == None {
            return None;
        }

        let (score, matched_range) = matched_result.unwrap();

        let begin = *matched_range.get(0).unwrap_or(&0) as i64;
        let end = *matched_range.last().unwrap_or(&0) as i64;

        let rank = build_rank(-score, item.get_index() as i64, begin, end);

        Some(
            MatchedItem::builder(item)
                .rank(rank)
                .matched_range(MatchedRange::Chars(matched_range))
                .build(),
        )
    }

    fn display(&self) -> String {
        format!("(Fuzzy: {})", self.query)
    }
}

//------------------------------------------------------------------------------
#[derive(Debug)]
struct MatchAllEngine {}

impl MatchAllEngine {
    pub fn builder() -> Self {
        Self {}
    }

    pub fn build(self) -> Self {
        self
    }
}

impl MatchEngine for MatchAllEngine {
    fn match_item(&self, item: Arc<ItemWrapper>) -> Option<MatchedItem> {
        let rank = build_rank(0, item.get_index() as i64, 0, 0);

        Some(
            MatchedItem::builder(item)
                .rank(rank)
                .matched_range(MatchedRange::ByteRange(0, 0))
                .build(),
        )
    }

    fn display(&self) -> String {
        "Noop".to_string()
    }
}

//------------------------------------------------------------------------------
// Exact engine
#[derive(Debug, Copy, Clone, Default)]
struct ExactMatchingParam {
    prefix: bool,
    postfix: bool,
    inverse: bool,
}

#[derive(Debug)]
struct ExactEngine {
    query: String,
    query_chars: Vec<char>,
    param: ExactMatchingParam,
}

impl ExactEngine {
    pub fn builder(query: &str, param: ExactMatchingParam) -> Self {
        ExactEngine {
            query: query.to_string(),
            query_chars: query.chars().collect(),
            param,
        }
    }

    pub fn build(self) -> Self {
        self
    }

    fn match_item_exact(
        &self,
        item: Arc<ItemWrapper>,
        // filter: <Option<(start, end), (start, end)>, item_length> -> Option<(start, end)>
        filter: impl Fn(&Option<((usize, usize), (usize, usize))>, usize) -> Option<(usize, usize)>,
    ) -> Option<MatchedItem> {
        let mut matched_result = None;
        let mut range_start = 0;
        let mut range_end = 0;
        for &(start, end) in item.get_matching_ranges().as_ref() {
            if self.query == "" {
                matched_result = Some(((0, 0), (0, 0)));
                break;
            }

            matched_result = score::exact_match(&item.get_text()[start..end], &self.query);

            if matched_result.is_some() {
                range_start = start;
                range_end = end;
                break;
            }
        }

        let (s, e) = filter(&matched_result, range_end - range_start)?;

        let (begin, end) = (s + range_start, e + range_start);
        let score = (end - begin) as i64;
        let rank = build_rank(-score, item.get_index() as i64, begin as i64, end as i64);

        Some(
            MatchedItem::builder(item)
                .rank(rank)
                .matched_range(MatchedRange::ByteRange(begin, end))
                .build(),
        )
    }
}

impl MatchEngine for ExactEngine {
    fn match_item(&self, item: Arc<ItemWrapper>) -> Option<MatchedItem> {
        self.match_item_exact(item, |match_result, len| {
            let match_range = match *match_result {
                Some(((s1, e1), (s2, e2))) => {
                    if self.param.prefix && self.param.postfix {
                        if s1 == 0 && e2 == len {
                            Some((s1, e1))
                        } else {
                            None
                        }
                    } else if self.param.prefix {
                        if s1 == 0 {
                            Some((s1, e1))
                        } else {
                            None
                        }
                    } else if self.param.postfix {
                        if e2 == len {
                            Some((s2, e2))
                        } else {
                            None
                        }
                    } else {
                        Some((s1, e1))
                    }
                }
                None => None,
            };

            if self.param.inverse {
                if match_range.is_some() {
                    None
                } else {
                    Some((0, 0))
                }
            } else {
                match_range
            }
        })
    }

    fn display(&self) -> String {
        format!(
            "(Exact|{}{}{}: {})",
            if self.param.inverse { "!" } else { "" },
            if self.param.prefix { "^" } else { "" },
            if self.param.postfix { "$" } else { "" },
            self.query
        )
    }
}

//------------------------------------------------------------------------------
// OrEngine, a combinator
struct OrEngine {
    engines: Vec<Box<dyn MatchEngine>>,
}

impl OrEngine {
    pub fn builder(query: &str, mode: MatcherMode, fuzzy_algorithm: FuzzyAlgorithm) -> Self {
        // mock
        OrEngine {
            engines: RE_OR
                .split(query)
                .map(|q| EngineFactory::build(q, mode, fuzzy_algorithm))
                .collect(),
        }
    }

    pub fn build(self) -> Self {
        self
    }
}

impl MatchEngine for OrEngine {
    fn match_item(&self, item: Arc<ItemWrapper>) -> Option<MatchedItem> {
        for engine in &self.engines {
            let result = engine.match_item(Arc::clone(&item));
            if result.is_some() {
                return result;
            }
        }

        None
    }

    fn display(&self) -> String {
        format!(
            "(Or: {})",
            self.engines.iter().map(|e| e.display()).collect::<Vec<_>>().join(", ")
        )
    }
}

//------------------------------------------------------------------------------
// AndEngine, a combinator
struct AndEngine {
    engines: Vec<Box<dyn MatchEngine>>,
}

impl AndEngine {
    pub fn builder(query: &str, mode: MatcherMode, fuzzy_algorithm: FuzzyAlgorithm) -> Self {
        let query_trim = query.trim_matches(|c| c == ' ' || c == '|');
        let mut engines = vec![];
        let mut last = 0;
        for mat in RE_AND.find_iter(query_trim) {
            let (start, end) = (mat.start(), mat.end());
            let term = &query_trim[last..start].trim_matches(|c| c == ' ' || c == '|');
            if !term.is_empty() {
                engines.push(EngineFactory::build(term, mode, fuzzy_algorithm));
            }

            if !mat.as_str().trim().is_empty() {
                engines.push(Box::new(
                    OrEngine::builder(mat.as_str().trim(), mode, fuzzy_algorithm).build(),
                ));
            }
            last = end;
        }

        let term = &query_trim[last..].trim_matches(|c| c == ' ' || c == '|');
        if !term.is_empty() {
            engines.push(EngineFactory::build(term, mode, fuzzy_algorithm));
        }

        AndEngine { engines }
    }

    pub fn build(self) -> Self {
        self
    }

    fn merge_matched_items(&self, items: Vec<MatchedItem>) -> MatchedItem {
        let rank = items[0].rank;
        let item = Arc::clone(&items[0].item);
        let mut ranges = vec![];
        for item in items {
            match item.matched_range {
                Some(MatchedRange::ByteRange(..)) => {
                    ranges.extend(item.range_char_indices().unwrap());
                }
                Some(MatchedRange::Chars(vec)) => {
                    ranges.extend(vec.iter());
                }
                _ => {}
            }
        }

        ranges.sort();
        ranges.dedup();
        MatchedItem::builder(item)
            .rank(rank)
            .matched_range(MatchedRange::Chars(ranges))
            .build()
    }
}

impl MatchEngine for AndEngine {
    fn match_item(&self, item: Arc<ItemWrapper>) -> Option<MatchedItem> {
        // mock
        let mut results = vec![];
        for engine in &self.engines {
            let result = engine.match_item(Arc::clone(&item))?;
            results.push(result);
        }

        if results.is_empty() {
            None
        } else {
            Some(self.merge_matched_items(results))
        }
    }

    fn display(&self) -> String {
        format!(
            "(And: {})",
            self.engines.iter().map(|e| e.display()).collect::<Vec<_>>().join(", ")
        )
    }
}

//------------------------------------------------------------------------------
pub struct EngineFactory {}
impl EngineFactory {
    pub fn build(query: &str, mode: MatcherMode, fuzzy_algorithm: FuzzyAlgorithm) -> Box<dyn MatchEngine> {
        match mode {
            MatcherMode::Regex => Box::new(RegexEngine::builder(query).build()),
            MatcherMode::Fuzzy | MatcherMode::Exact => {
                if query.contains(' ') {
                    Box::new(AndEngine::builder(query, mode, fuzzy_algorithm).build())
                } else {
                    EngineFactory::build_single(query, mode, fuzzy_algorithm)
                }
            }
        }
    }

    fn build_single(query: &str, mode: MatcherMode, fuzzy_algorithm: FuzzyAlgorithm) -> Box<dyn MatchEngine> {
        // 'abc => match exact "abc"
        // ^abc => starts with "abc"
        // abc$ => ends with "abc"
        // ^abc$ => match exact "abc"
        // !^abc => items not starting with "abc"
        // !abc$ => items not ending with "abc"
        // !^abc$ => not "abc"

        let mut query = query;
        let mut exact = false;
        let mut param = ExactMatchingParam::default();

        if query.starts_with('\'') {
            if mode == MatcherMode::Exact {
                return Box::new(FuzzyEngine::builder(&query[1..]).algorithm(fuzzy_algorithm).build());
            } else {
                return Box::new(ExactEngine::builder(&query[1..], param).build());
            }
        }

        if query.starts_with('!') {
            query = &query[1..];
            exact = true;
            param.inverse = true;
        }

        if query.is_empty() {
            // if only "!" was provided, will still show all items
            return Box::new(MatchAllEngine::builder().build());
        }

        if query.starts_with('^') {
            query = &query[1..];
            exact = true;
            param.prefix = true;
        }

        if query.ends_with('$') {
            query = &query[..(query.len() - 1)];
            exact = true;
            param.postfix = true;
        }

        if mode == MatcherMode::Exact {
            exact = true;
        }

        if exact {
            Box::new(ExactEngine::builder(query, param).build())
        } else {
            Box::new(FuzzyEngine::builder(query).algorithm(fuzzy_algorithm).build())
        }
    }
}

#[cfg(test)]
mod test {
    use super::{EngineFactory, MatcherMode};
    use crate::engine::FuzzyAlgorithm;

    #[test]
    fn test_engine_factory() {
        let x = EngineFactory::build("'abc", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(Exact|: abc)");

        let x = EngineFactory::build("^abc", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(Exact|^: abc)");

        let x = EngineFactory::build("abc$", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(Exact|$: abc)");

        let x = EngineFactory::build("^abc$", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(Exact|^$: abc)");

        let x = EngineFactory::build("!abc", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(Exact|!: abc)");

        let x = EngineFactory::build("!^abc", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(Exact|!^: abc)");

        let x = EngineFactory::build("!abc$", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(Exact|!$: abc)");

        let x = EngineFactory::build("!^abc$", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(Exact|!^$: abc)");

        let x = EngineFactory::build(
            "'abc | def ^gh ij | kl mn",
            MatcherMode::Fuzzy,
            FuzzyAlgorithm::default(),
        );

        assert_eq!(
            x.display(),
            "(And: (Or: (Exact|: abc), (Fuzzy: def)), (Exact|^: gh), (Or: (Fuzzy: ij), (Fuzzy: kl)), (Fuzzy: mn))"
        );

        let x = EngineFactory::build(
            "'abc | def ^gh ij | kl mn",
            MatcherMode::Regex,
            FuzzyAlgorithm::default(),
        );
        assert_eq!(x.display(), "(Regex: 'abc | def ^gh ij | kl mn)");

        let x = EngineFactory::build("abc ", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(And: (Fuzzy: abc))");

        let x = EngineFactory::build("abc def", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(And: (Fuzzy: abc), (Fuzzy: def))");

        let x = EngineFactory::build("abc | def", MatcherMode::Fuzzy, FuzzyAlgorithm::default());
        assert_eq!(x.display(), "(And: (Or: (Fuzzy: abc), (Fuzzy: def)))");
    }
}

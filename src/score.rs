///! score is responsible for calculating the scores of the similarity between
///! the query and the choice.
use fuzzy_matcher::clangd::ClangdMatcher;
use fuzzy_matcher::skim::{SkimMatcher, SkimMatcherV2};
use fuzzy_matcher::FuzzyMatcher;
use regex::Regex;

#[derive(Debug, Copy, Clone)]
pub enum FuzzyAlgorithm {
    SkimV1,
    SkimV2,
    Clangd,
}

impl FuzzyAlgorithm {
    pub fn of(algorithm: &str) -> Self {
        match algorithm.to_ascii_lowercase().as_ref() {
            "skim_v1" => FuzzyAlgorithm::SkimV1,
            "skim_v2" | "skim" => FuzzyAlgorithm::SkimV2,
            "clangd" => FuzzyAlgorithm::Clangd,
            _ => FuzzyAlgorithm::SkimV2,
        }
    }
}

impl Default for FuzzyAlgorithm {
    fn default() -> Self {
        FuzzyAlgorithm::SkimV2
    }
}

const BYTES_1M: usize = 1024 * 1024 * 1024;

lazy_static! {
    static ref SKIM_V1: SkimMatcher = SkimMatcher::default();
    static ref SKIM_V2: SkimMatcherV2 = SkimMatcherV2::default().element_limit(BYTES_1M);
    static ref CLANGD: ClangdMatcher = ClangdMatcher::default();
}

pub fn fuzzy_match(choice: &str, pattern: &str, fuzzy_algorithm: FuzzyAlgorithm) -> Option<(i64, Vec<usize>)> {
    if pattern.is_empty() {
        return Some((0, Vec::new()));
    } else if choice.is_empty() {
        return None;
    }

    match fuzzy_algorithm {
        FuzzyAlgorithm::SkimV1 => SKIM_V1.fuzzy_indices(choice, pattern),
        FuzzyAlgorithm::SkimV2 => SKIM_V2.fuzzy_indices(choice, pattern),
        FuzzyAlgorithm::Clangd => CLANGD.fuzzy_indices(choice, pattern),
    }
}

pub fn regex_match(choice: &str, pattern: &Option<Regex>) -> Option<(usize, usize)> {
    match *pattern {
        Some(ref pat) => {
            let mat = pat.find(choice)?;
            Some((mat.start(), mat.end()))
        }
        None => None,
    }
}

// Pattern may appear in several places, return the first and last occurrence
pub fn exact_match(choice: &str, pattern: &str) -> Option<((usize, usize), (usize, usize))> {
    // search from the start
    let start_pos = choice.find(pattern)?;
    let first_occur = (start_pos, start_pos + pattern.len());
    let last_pos = choice.rfind(pattern)?;
    let last_occur = (last_pos, last_pos + pattern.len());
    Some((first_occur, last_occur))
}

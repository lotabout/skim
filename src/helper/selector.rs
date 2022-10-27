use std::collections::HashSet;

use regex::Regex;

use crate::{Selector, SkimItem};

#[derive(Debug, Default)]
pub struct DefaultSkimSelector {
    first_n: usize,
    regex: Option<Regex>,
    preset: Option<HashSet<String>>,
}

impl DefaultSkimSelector {
    pub fn first_n(mut self, first_n: usize) -> Self {
        trace!("select first_n: {}", first_n);
        self.first_n = first_n;
        self
    }

    pub fn preset(mut self, preset: impl IntoIterator<Item = String>) -> Self {
        if self.preset.is_none() {
            self.preset = Some(HashSet::new())
        }

        if let Some(set) = self.preset.as_mut() {
            set.extend(preset)
        }
        self
    }

    pub fn regex(mut self, regex: &str) -> Self {
        trace!("select regex: {}", regex);
        if !regex.is_empty() {
            self.regex = Regex::new(regex).ok();
        }
        self
    }
}

impl Selector for DefaultSkimSelector {
    fn should_select(&self, index: usize, item: &dyn SkimItem) -> bool {
        if self.first_n > index {
            return true;
        }

        if self.preset.is_some()
            && self
                .preset
                .as_ref()
                .map(|preset| preset.contains(item.text().as_ref()))
                .unwrap_or(false)
        {
            return true;
        }

        if self.regex.is_some() && self.regex.as_ref().map(|re| re.is_match(&item.text())).unwrap_or(false) {
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn test_first_n() {
        let selector = DefaultSkimSelector::default().first_n(10);
        assert!(selector.should_select(0, &"item"));
        assert!(selector.should_select(1, &"item"));
        assert!(selector.should_select(2, &"item"));
        assert!(selector.should_select(9, &"item"));
        assert!(!selector.should_select(10, &"item"));
    }

    #[test]
    pub fn test_preset() {
        let selector = DefaultSkimSelector::default().preset(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        assert!(selector.should_select(0, &"a"));
        assert!(selector.should_select(0, &"b"));
        assert!(selector.should_select(0, &"c"));
        assert!(!selector.should_select(0, &"d"));
    }

    #[test]
    pub fn test_regex() {
        let selector = DefaultSkimSelector::default().regex("^[0-9]");
        assert!(selector.should_select(0, &"1"));
        assert!(selector.should_select(0, &"2"));
        assert!(selector.should_select(0, &"3"));
        assert!(selector.should_select(0, &"1a"));
        assert!(!selector.should_select(0, &"a"));
    }

    #[test]
    pub fn test_all_together() {
        let selector = DefaultSkimSelector::default()
            .first_n(1)
            .regex("b")
            .preset(vec!["c".to_string()]);
        assert!(selector.should_select(0, &"a"));
        assert!(selector.should_select(1, &"b"));
        assert!(selector.should_select(2, &"c"));
        assert!(!selector.should_select(3, &"d"));
    }
}

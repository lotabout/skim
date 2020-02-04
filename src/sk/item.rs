use crate::ansi::{ANSIParser, AnsiString};
use crate::field::{parse_matching_fields, parse_transform_fields, FieldRange};
use crate::SkimItem;
use regex::Regex;
use std::borrow::Cow;

/// An item will store everything that one line input will need to be operated and displayed.
///
/// What's special about an item?
/// The simplest version of an item is a line of string, but things are getting more complex:
/// - The conversion of lower/upper case is slow in rust, because it involds unicode.
/// - We may need to interpret the ANSI codes in the text.
/// - The text can be transformed and limited while searching.
///
/// About the ANSI, we made assumption that it is linewise, that means no ANSI codes will affect
/// more than one line.
#[derive(Debug)]
pub struct SkItem {
    // The text that will be ouptut when user press `enter`
    orig_text: String,

    // The text that will shown into the screen. Can be transformed.
    text: AnsiString<'static>,

    matching_ranges: Vec<(usize, usize)>,

    // For the transformed ANSI case, the output will need another transform.
    using_transform_fields: bool,
    ansi_enabled: bool,
}

impl<'a> SkItem {
    pub fn new(
        orig_text: Cow<str>,
        ansi_enabled: bool,
        trans_fields: &[FieldRange],
        matching_fields: &[FieldRange],
        delimiter: &Regex,
    ) -> Self {
        let using_transform_fields = !trans_fields.is_empty();

        //        transformed | ANSI             | output
        //------------------------------------------------------
        //                    +- T -> trans+ANSI | ANSI
        //                    |                  |
        //      +- T -> trans +- F -> trans      | orig
        // orig |                                |
        //      +- F -> orig  +- T -> ANSI     ==| ANSI
        //                    |                  |
        //                    +- F -> orig       | orig

        let mut ansi_parser: ANSIParser = Default::default();

        let text = if using_transform_fields && ansi_enabled {
            // ansi and transform
            ansi_parser.parse_ansi(&parse_transform_fields(delimiter, &orig_text, trans_fields))
        } else if using_transform_fields {
            // transformed, not ansi
            AnsiString::new_string(parse_transform_fields(delimiter, &orig_text, trans_fields))
        } else if ansi_enabled {
            // not transformed, ansi
            ansi_parser.parse_ansi(&orig_text)
        } else {
            // normal case
            AnsiString::new_empty()
        };

        let mut ret = SkItem {
            orig_text: orig_text.into(),
            text,
            using_transform_fields: !trans_fields.is_empty(),
            matching_ranges: Vec::new(),
            ansi_enabled,
        };

        let matching_ranges = if !matching_fields.is_empty() {
            parse_matching_fields(delimiter, &ret.get_text(), matching_fields)
        } else {
            vec![(0, ret.get_text().len())]
        };

        ret.matching_ranges = matching_ranges;
        ret
    }
}

impl SkimItem for SkItem {
    fn display(&self) -> Cow<AnsiString> {
        if self.using_transform_fields || self.ansi_enabled {
            Cow::Borrowed(&self.text)
        } else {
            Cow::Owned(AnsiString::new_str(&self.orig_text))
        }
    }

    fn get_text(&self) -> Cow<str> {
        if !self.using_transform_fields && !self.ansi_enabled {
            Cow::Borrowed(&self.orig_text)
        } else {
            Cow::Borrowed(self.text.stripped())
        }
    }

    fn output(&self) -> Cow<str> {
        if self.using_transform_fields && self.ansi_enabled {
            let mut ansi_parser: ANSIParser = Default::default();
            let text = ansi_parser.parse_ansi(&self.orig_text);
            Cow::Owned(text.into_inner())
        } else if !self.using_transform_fields && self.ansi_enabled {
            Cow::Borrowed(self.text.stripped())
        } else {
            Cow::Borrowed(&self.orig_text)
        }
    }

    fn get_matching_ranges(&self) -> Cow<[(usize, usize)]> {
        Cow::Borrowed(&self.matching_ranges)
    }
}

impl Clone for SkItem {
    fn clone(&self) -> SkItem {
        SkItem {
            orig_text: self.orig_text.clone(),
            text: self.text.clone(),
            using_transform_fields: self.using_transform_fields,
            matching_ranges: self.matching_ranges.clone(),
            ansi_enabled: self.ansi_enabled,
        }
    }
}

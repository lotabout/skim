extern crate skim;
use std::default::Default;
use skim::{Skim, SkimOptionsBuilder, SkimOptions};
use std::io::{self, BufRead, Cursor};
#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref OPTIONS: SkimOptions<'static> = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(true)
        .build()
        .unwrap();
}

// This example is not working for now cause skim did not release resources correctly
pub fn main() {
    let input = "aaaaa\nbbbb\nccc".to_string();

    let selected_items = Skim::run_with(&OPTIONS, Some(Box::new(Cursor::new(input))))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}: {}{}", item.item.get_index(), item.item.get_output_text(), "\n");
    }

    let input = "11111\n22222\n333333333".to_string();

    let selected_items = Skim::run_with(&OPTIONS, Some(Box::new(Cursor::new(input))))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}: {}{}", item.item.get_index(), item.item.get_output_text(), "\n");
    }
}

extern crate skim;
use std::default::Default;
use skim::{Skim, SkimOptions};
use std::io::Cursor;
#[macro_use]
extern crate lazy_static;

lazy_static! {
}

pub fn main() {
    let options: SkimOptions = SkimOptions::default()
        .height("50%")
        .multi(true);

    //==================================================
    // first run
    let input = "aaaaa\nbbbb\nccc".to_string();

    let selected_items = Skim::run_with(&options, Some(Box::new(Cursor::new(input))))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}: {}{}", item.item.get_index(), item.item.get_output_text(), "\n");
    }

    //==================================================
    // second run
    let input = "11111\n22222\n333333333".to_string();

    let selected_items = Skim::run_with(&options, Some(Box::new(Cursor::new(input))))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}: {}{}", item.item.get_index(), item.item.get_output_text(), "\n");
    }
}

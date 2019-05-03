extern crate skim;
use skim::{Skim, SkimOptionsBuilder};
use std::io::Cursor;

pub fn main() {
    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(true)
        .build()
        .unwrap();

    //==================================================
    // first run
    let input = "aaaaa\nbbbb\nccc".to_string();

    let selected_items = Skim::run_with(&options, Some(Box::new(Cursor::new(input))))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}: {}{}", item.get_index(), item.get_output_text(), "\n");
    }

    //==================================================
    // second run
    let input = "11111\n22222\n333333333".to_string();

    let selected_items = Skim::run_with(&options, Some(Box::new(Cursor::new(input))))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}: {}{}", item.get_index(), item.get_output_text(), "\n");
    }
}

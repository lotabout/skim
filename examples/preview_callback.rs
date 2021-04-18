use std::io::Cursor;

use skim::prelude::*;

pub fn main() {
    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(true)
        .preview_fn(|items: Vec<String>| {
            items.iter()
                 .map(|s| s.to_ascii_uppercase().into())
                 .collect::<Vec<_>>()
        })
        .build()
        .unwrap();
    let item_reader = SkimItemReader::default();

    let input = "aaaaa\nbbbb\nccc";
    let items = item_reader.of_bufread(Cursor::new(input));
    let selected_items = Skim::run_with(&options, Some(items))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}{}", item.output(), "\n");
    }
}

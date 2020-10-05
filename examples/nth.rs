extern crate skim;
use skim::prelude::*;
use std::io::Cursor;

pub fn main() {
    let input = "foo 123";

    let options = SkimOptionsBuilder::default().query(Some("f")).build().unwrap();
    let item_reader = SkimItemReader::new(SkimItemReaderOption::default().nth("2").build());

    let items = item_reader.of_bufread(Cursor::new(input));
    let selected_items = Skim::run_with(&options, Some(items))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}{}", item.output(), "\n");
    }
}

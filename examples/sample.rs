extern crate skim;
use skim::{Skim, SkimOptions};

pub fn main() {
    let options = SkimOptions::default();

    let selected_items = Skim::run_with(&options, None)
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}{}", item.get_output_text(), "\n");
    }
}

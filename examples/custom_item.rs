extern crate skim;
use crossbeam::channel::unbounded;
use skim::prelude::*;
use std::borrow::Cow;
use std::sync::Arc;

struct MyItem {
    inner: String,
}

impl SkimItem for MyItem {
    fn display(&self) -> Cow<AnsiString> {
        Cow::Owned(AnsiString::new_str(&self.inner))
    }

    fn get_text(&self) -> Cow<str> {
        Cow::Borrowed(&self.inner)
    }

    fn preview(&self) -> ItemPreview {
        ItemPreview::Text(format!("hello, {}", self.inner))
    }
}

pub fn main() {
    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(true)
        .preview(Some("")) // preview should be specified to enable preview window
        .build()
        .unwrap();

    let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) = unbounded();
    let _ = tx_item.send(Arc::new(MyItem {
        inner: "aaaaa".to_string(),
    }));
    let _ = tx_item.send(Arc::new(MyItem {
        inner: "bbbb".to_string(),
    }));
    let _ = tx_item.send(Arc::new(MyItem {
        inner: "ccc".to_string(),
    }));
    drop(tx_item); // so that skim could know when to stop waiting for more items.

    let selected_items = Skim::run_with(&options, Some(rx_item))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}{}", item.output(), "\n");
    }
}

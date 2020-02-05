extern crate skim;
use crossbeam::channel::unbounded;
use skim::prelude::*;
use std::sync::Arc;

pub fn main() {
    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(true)
        .build()
        .unwrap();

    //==================================================
    // first run
    let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) = unbounded();
    let _ = tx_item.send(Arc::new("aaaaa"));
    let _ = tx_item.send(Arc::new("bbbb"));
    let _ = tx_item.send(Arc::new("ccc"));
    drop(tx_item);

    let selected_items = Skim::run_with(&options, Some(rx_item))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}{}", item.output(), "\n");
    }

    //==================================================
    // second run
    let (tx_item, rx_item): (SkimItemSender, SkimItemReceiver) = unbounded();
    let _ = tx_item.send(Arc::new("11111"));
    let _ = tx_item.send(Arc::new("22222"));
    let _ = tx_item.send(Arc::new("333333333"));
    drop(tx_item);

    let selected_items = Skim::run_with(&options, Some(rx_item))
        .map(|out| out.selected_items)
        .unwrap_or_else(|| Vec::new());

    for item in selected_items.iter() {
        print!("{}{}", item.output(), "\n");
    }
}

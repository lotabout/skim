extern crate skim;
use skim::prelude::*;

/// This example illustrates downcasting custom structs that implement
/// `SkimItem` after calling `Skim::run_with`.

#[derive(Debug, Clone)]
struct Item {
    text: String,
}

impl SkimItem for Item {
    fn text(&self) -> Cow<str> {
        Cow::Borrowed(&self.text)
    }

    fn preview(&self, _context: PreviewContext) -> ItemPreview {
        ItemPreview::Text(self.text.to_owned())
    }
}

pub fn main() {
    let options = SkimOptionsBuilder::default()
        .height(Some("50%"))
        .multi(true)
        .preview(Some(""))
        .build()
        .unwrap();

    let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();

    tx.send(Arc::new(Item { text: "a".to_string() })).unwrap();
    tx.send(Arc::new(Item { text: "b".to_string() })).unwrap();
    tx.send(Arc::new(Item { text: "c".to_string() })).unwrap();

    drop(tx);

    let selected_items = Skim::run_with(&options, Some(rx))
        .map(|out| out.selected_items)
        .unwrap_or_else(Vec::new)
        .into_iter()
        .map(|selected_item| Item {
            text: selected_item.text().to_string(),
        })
        .collect::<Vec<Item>>();

    for item in selected_items {
        println!("{:?}", item);
    }
}

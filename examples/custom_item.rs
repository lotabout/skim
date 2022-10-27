extern crate skim;
use skim::prelude::*;

struct MyItem {
    inner: String,
}

impl SkimItem for MyItem {
    fn text(&self) -> Cow<str> {
        Cow::Borrowed(&self.inner)
    }

    fn preview(&self, _context: PreviewContext) -> ItemPreview {
        if self.inner.starts_with("color") {
            ItemPreview::AnsiText(format!("\x1b[31mhello:\x1b[m\n{}", self.inner))
        } else {
            ItemPreview::Text(format!("hello:\n{}", self.inner))
        }
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
        inner: "color aaaa".to_string(),
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
        .unwrap_or_else(Vec::new);

    for item in selected_items.iter() {
        println!("{}", item.output());
    }
}

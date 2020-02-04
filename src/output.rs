use crate::item::ItemWrapper;
use std::sync::Arc;

pub struct SkimOutput {
    pub accept_key: Option<String>,
    pub query: String,
    pub cmd: String,
    pub selected_items: Vec<Arc<ItemWrapper>>,
}

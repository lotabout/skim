use crate::event::Event;
use crate::SkimItem;
use std::sync::Arc;
use tuikit::key::Key;

pub struct SkimOutput {
    /// The final event that makes skim accept/quit.
    /// Was designed to determine if skim quit or accept.
    /// Typically there are only two options: `Event::EvActAbort` | `Event::EvActAccept`
    pub final_event: Event,

    /// quick pass for judging if skim aborts.
    pub is_abort: bool,

    /// The final key that makes skim accept/quit.
    /// Note that it might be Key::Null if it is triggered by skim.
    pub final_key: Key,

    /// The query
    pub query: String,

    /// The command query
    pub cmd: String,

    /// The selected items.
    pub selected_items: Vec<Arc<dyn SkimItem>>,
}

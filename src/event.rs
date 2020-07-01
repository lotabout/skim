// All the events that will be used

use bitflags::bitflags;
use std::sync::mpsc::{Receiver, Sender};
use tuikit::key::Key;

pub type EventReceiver = Receiver<Event>;
pub type EventSender = Sender<Event>;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Event {
    EvInputKey(Key),
    EvInputInvalid,
    EvHeartBeat,

    // user bind actions
    EvActAbort,
    EvActAccept(Option<String>),
    EvActAddChar(char),
    EvActAppendAndSelect,
    EvActBackwardChar,
    EvActBackwardDeleteChar,
    EvActBackwardKillWord,
    EvActBackwardWord,
    EvActBeginningOfLine,
    EvActCancel,
    EvActClearScreen,
    EvActDeleteChar,
    EvActDeleteCharEOF,
    EvActDeselectAll,
    EvActDown(i32),
    EvActEndOfLine,
    EvActExecute(String),
    EvActExecuteSilent(String),
    EvActForwardChar,
    EvActForwardWord,
    EvActIfQueryEmpty(String),
    EvActIfQueryNotEmpty(String),
    EvActIfNonMatched(String),
    EvActIgnore,
    EvActKillLine,
    EvActKillWord,
    EvActNextHistory,
    EvActHalfPageDown(i32),
    EvActHalfPageUp(i32),
    EvActPageDown(i32),
    EvActPageUp(i32),
    EvActPreviewUp(i32),
    EvActPreviewDown(i32),
    EvActPreviewLeft(i32),
    EvActPreviewRight(i32),
    EvActPreviewPageUp(i32),
    EvActPreviewPageDown(i32),
    EvActPreviousHistory,
    EvActRedraw,
    EvActRotateMode,
    EvActScrollLeft(i32),
    EvActScrollRight(i32),
    EvActSelectAll,
    EvActSelectRow(usize),
    EvActToggle,
    EvActToggleAll,
    EvActToggleIn,
    EvActToggleInteractive,
    EvActToggleOut,
    EvActTogglePreview,
    EvActTogglePreviewWrap,
    EvActToggleSort,
    EvActUnixLineDiscard,
    EvActUnixWordRubout,
    EvActUp(i32),
    EvActYank,
}

bitflags! {
    /// `Effect` is the effect of a text
    pub struct UpdateScreen: u8 {
        const REDRAW = 0b0000_0000;
        const DONT_REDRAW = 0b0000_0010;
    }
}

pub trait EventHandler {
    /// handle event, return whether
    fn handle(&mut self, event: &Event) -> UpdateScreen;
}

#[rustfmt::skip]
pub fn parse_event(action: &str, arg: Option<String>) -> Option<Event> {
    match action {
        "abort"                =>   Some(Event::EvActAbort),
        "accept"               =>   Some(Event::EvActAccept(arg)),
        "append-and-select"    =>   Some(Event::EvActAppendAndSelect),
        "backward-char"        =>   Some(Event::EvActBackwardChar),
        "backward-delete-char" =>   Some(Event::EvActBackwardDeleteChar),
        "backward-kill-word"   =>   Some(Event::EvActBackwardKillWord),
        "backward-word"        =>   Some(Event::EvActBackwardWord),
        "beginning-of-line"    =>   Some(Event::EvActBeginningOfLine),
        "cancel"               =>   Some(Event::EvActCancel),
        "clear-screen"         =>   Some(Event::EvActClearScreen),
        "delete-char"          =>   Some(Event::EvActDeleteChar),
        "delete-charEOF"       =>   Some(Event::EvActDeleteCharEOF),
        "deselect-all"         =>   Some(Event::EvActDeselectAll),
        "down"                 =>   Some(Event::EvActDown(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "end-of-line"          =>   Some(Event::EvActEndOfLine),
        "execute"              =>   Some(Event::EvActExecute(arg.expect("execute event should have argument"))),
        "execute-silent"       =>   Some(Event::EvActExecuteSilent(arg.expect("execute-silent event should have argument"))),
        "forward-char"         =>   Some(Event::EvActForwardChar),
        "forward-word"         =>   Some(Event::EvActForwardWord),
        "if-non-matched"       =>   Some(Event::EvActIfNonMatched(arg.expect("no arg specified for event if-non-matched"))),
        "if-query-empty"       =>   Some(Event::EvActIfQueryEmpty(arg.expect("no arg specified for event if-query-empty"))),
        "if-query-not-empty"   =>   Some(Event::EvActIfQueryNotEmpty(arg.expect("no arg specified for event if-query-not-empty"))),
        "ignore"               =>   Some(Event::EvActIgnore),
        "kill-line"            =>   Some(Event::EvActKillLine),
        "kill-word"            =>   Some(Event::EvActKillWord),
        "next-history"         =>   Some(Event::EvActNextHistory),
        "half-page-down"       =>   Some(Event::EvActHalfPageDown(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "half-page-up"         =>   Some(Event::EvActHalfPageUp(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "page-down"            =>   Some(Event::EvActPageDown(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "page-up"              =>   Some(Event::EvActPageUp(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "preview-up"           =>   Some(Event::EvActPreviewUp(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "preview-down"         =>   Some(Event::EvActPreviewDown(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "preview-left"         =>   Some(Event::EvActPreviewLeft(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "preview-right"        =>   Some(Event::EvActPreviewRight(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "preview-page-up"      =>   Some(Event::EvActPreviewPageUp(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "preview-page-down"    =>   Some(Event::EvActPreviewPageDown(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "previous-history"     =>   Some(Event::EvActPreviousHistory),
        "scroll-left"          =>   Some(Event::EvActScrollLeft(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "scroll-right"         =>   Some(Event::EvActScrollRight(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "select-all"           =>   Some(Event::EvActSelectAll),
        "toggle"               =>   Some(Event::EvActToggle),
        "toggle-all"           =>   Some(Event::EvActToggleAll),
        "toggle-in"            =>   Some(Event::EvActToggleIn),
        "toggle-interactive"   =>   Some(Event::EvActToggleInteractive),
        "toggle-out"           =>   Some(Event::EvActToggleOut),
        "toggle-preview"       =>   Some(Event::EvActTogglePreview),
        "toggle-preview-wrap"  =>   Some(Event::EvActTogglePreviewWrap),
        "toggle-sort"          =>   Some(Event::EvActToggleSort),
        "unix-line-discard"    =>   Some(Event::EvActUnixLineDiscard),
        "unix-word-rubout"     =>   Some(Event::EvActUnixWordRubout),
        "up"                   =>   Some(Event::EvActUp(arg.and_then(|s|s.parse().ok()).unwrap_or(1))),
        "yank"                 =>   Some(Event::EvActYank),
        _ => None
    }
}

// All the events that will be used

use bitflags::bitflags;
use std::any::Any;
use std::sync::mpsc::{Receiver, Sender};

pub type EventArg = Box<Any + 'static + Send>;
pub type EventReceiver = Receiver<(Event, EventArg)>;
pub type EventSender = Sender<(Event, EventArg)>;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Event {
    EvInputKey,
    EvInputInvalid,

    EvHeartBeat,

    EvPreviewRequest,

    // user bind actions
    EvActAbort,
    EvActAccept,
    EvActAddChar,
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
    EvActDown,
    EvActEndOfLine,
    EvActExecute,
    EvActExecuteSilent,
    EvActForwardChar,
    EvActForwardWord,
    EvActIgnore,
    EvActKillLine,
    EvActKillWord,
    EvActNextHistory,
    EvActPageDown,
    EvActPageUp,
    EvActPreviewUp,
    EvActPreviewDown,
    EvActPreviewLeft,
    EvActPreviewRight,
    EvActPreviewPageUp,
    EvActPreviewPageDown,
    EvActPreviousHistory,
    EvActRedraw,
    EvActRotateMode,
    EvActScrollLeft,
    EvActScrollRight,
    EvActSelectAll,
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
    EvActUp,
    EvActYank,
}

bitflags! {
    /// `Effect` is the effect of a text
    pub struct UpdateScreen: u8 {
        const REDRAW = 0b00000000;
        const DONT_REDRAW = 0b00000010;
    }
}

pub trait EventHandler {
    fn accept_event(&self, event: Event) -> bool;

    /// handle event, return whether
    fn handle(&mut self, event: Event, arg: &EventArg) -> UpdateScreen;
}

#[rustfmt::skip]
pub fn parse_action(action: &str) -> Option<Event> {
    match action {
        "abort"                =>   Some(Event::EvActAbort),
        "accept"               =>   Some(Event::EvActAccept),
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
        "down"                 =>   Some(Event::EvActDown),
        "end-of-line"          =>   Some(Event::EvActEndOfLine),
        "execute"              =>   Some(Event::EvActExecute),
        "execute-silent"       =>   Some(Event::EvActExecuteSilent),
        "forward-char"         =>   Some(Event::EvActForwardChar),
        "forward-word"         =>   Some(Event::EvActForwardWord),
        "ignore"               =>   Some(Event::EvActIgnore),
        "kill-line"            =>   Some(Event::EvActKillLine),
        "kill-word"            =>   Some(Event::EvActKillWord),
        "next-history"         =>   Some(Event::EvActNextHistory),
        "page-down"            =>   Some(Event::EvActPageDown),
        "page-up"              =>   Some(Event::EvActPageUp),
        "preview-up"           =>   Some(Event::EvActPreviewUp),
        "preview-down"         =>   Some(Event::EvActPreviewDown),
        "preview-left"         =>   Some(Event::EvActPreviewLeft),
        "preview-right"        =>   Some(Event::EvActPreviewRight),
        "preview-page-up"      =>   Some(Event::EvActPreviewPageUp),
        "preview-page-down"    =>   Some(Event::EvActPreviewPageDown),
        "previous-history"     =>   Some(Event::EvActPreviousHistory),
        "scroll-left"          =>   Some(Event::EvActScrollLeft),
        "scroll-right"         =>   Some(Event::EvActScrollRight),
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
        "up"                   =>   Some(Event::EvActUp),
        "yank"                 =>   Some(Event::EvActYank),
        _ => None
    }
}

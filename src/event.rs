// All the events that will be used

use std::any::Any;
use std::sync::mpsc::{Receiver, Sender};

pub type EventArg = Box<Any + 'static + Send>;
pub type EventReceiver = Receiver<(Event, EventArg)>;
pub type EventSender = Sender<(Event, EventArg)>;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Event {
    EvInputKey,
    EvInputInvalid,

    EvModelNewPreview,

    EvMatcherDone,

    EvReaderNewItem,
    EvReaderStarted,
    EvReaderStopped,
    EvReaderRestart,
    EvHeartBeat,

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
    EvActForwardChar,
    EvActForwardWord,
    EvActIgnore,
    EvActKillLine,
    EvActKillWord,
    EvActNextHistory,
    EvActPageDown,
    EvActPageUp,
    EvActPreviousHistory,
    EvActRedraw,
    EvActRotateMode,
    EvActScrollLeft,
    EvActScrollRight,
    EvActSelectAll,
    EvActToggle,
    EvActToggleAll,
    EvActToggleDown,
    EvActToggleIn,
    EvActToggleInteractive,
    EvActToggleOut,
    EvActTogglePreview,
    EvActToggleSort,
    EvActToggleUp,
    EvActUnixLineDiscard,
    EvActUnixWordRubout,
    EvActUp,
    EvActYank,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum UpdateScreen {
    /// Redraw the screen
    Redraw,
    /// Don't redraw the screen
    DontRedraw,
}

pub trait EventHandler {
    fn accept_event(&self, event: Event) -> bool;

    /// handle event, return whether
    fn handle(&mut self, event: Event, arg: EventArg) -> UpdateScreen;
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
        "forward-char"         =>   Some(Event::EvActForwardChar),
        "forward-word"         =>   Some(Event::EvActForwardWord),
        "ignore"               =>   Some(Event::EvActIgnore),
        "kill-line"            =>   Some(Event::EvActKillLine),
        "kill-word"            =>   Some(Event::EvActKillWord),
        "next-history"         =>   Some(Event::EvActNextHistory),
        "page-down"            =>   Some(Event::EvActPageDown),
        "page-up"              =>   Some(Event::EvActPageUp),
        "previous-history"     =>   Some(Event::EvActPreviousHistory),
        "scroll-left"          =>   Some(Event::EvActScrollLeft),
        "scroll-right"         =>   Some(Event::EvActScrollRight),
        "select-all"           =>   Some(Event::EvActSelectAll),
        "toggle"               =>   Some(Event::EvActToggle),
        "toggle-all"           =>   Some(Event::EvActToggleAll),
        "toggle-down"          =>   Some(Event::EvActToggleDown),
        "toggle-in"            =>   Some(Event::EvActToggleIn),
        "toggle-interactive"   =>   Some(Event::EvActToggleInteractive),
        "toggle-out"           =>   Some(Event::EvActToggleOut),
        "toggle-preview"       =>   Some(Event::EvActTogglePreview),
        "toggle-sort"          =>   Some(Event::EvActToggleSort),
        "toggle-up"            =>   Some(Event::EvActToggleUp),
        "unix-line-discard"    =>   Some(Event::EvActUnixLineDiscard),
        "unix-word-rubout"     =>   Some(Event::EvActUnixWordRubout),
        "up"                   =>   Some(Event::EvActUp),
        "yank"                 =>   Some(Event::EvActYank),
        _ => None
    }
}

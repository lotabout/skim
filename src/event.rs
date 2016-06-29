// All the events that will be used

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Event {
    EvReaderNewItem,
    EvReaderFinished,
    EvMatcherNewItem,
    EvMatcherResetQuery,
    EvMatcherUpdateProcess,
    EvMatcherStart,
    EvMatcherStartReceived,
    EvMatcherEnd,
    EvQueryChange,
    EvInputKey,
    EvInputInvalid,
    EvResize,
    EvActAddChar,
    EvActToggleDown,
    EvActUp,
    EvActDown,
    EvActBackwardChar,
    EvActBackwardDeleteChar,
    EvActSelect,
    EvActQuit,
}

use std::error::Error;
use std::io::{BufRead, Read};
use std::os::fd::AsRawFd;
use std::os::unix::io::RawFd;
use std::time::Duration;

use nix::sys::select;
use nix::sys::time::{TimeVal, TimeValLike};
use crate::helper::sys_util::WaitState::{INTERRUPTED, READY, TIMEOUT};

fn duration_to_timeval(duration: Duration) -> TimeVal {
    let sec = duration.as_secs() * 1000 + (duration.subsec_millis() as u64);
    TimeVal::milliseconds(sec as i64)
}

#[derive(PartialEq)]
pub enum WaitState {
    READY,
    TIMEOUT,
    INTERRUPTED,
}


/// self-pipe trick to wait for a fd that could be waken up by another fd
pub fn wait_until_ready(fd: RawFd, signal_fd: Option<RawFd>, timeout: Duration) -> WaitState {
    let mut timeout_spec = if timeout == Duration::new(0, 0) {
        None
    } else {
        Some(duration_to_timeval(timeout))
    };

    let mut fdset = select::FdSet::new();
    fdset.insert(fd);
    signal_fd.map(|fd| fdset.insert(fd));
    let n = select::select(None, &mut fdset, None, None, &mut timeout_spec)
        .expect("error on select");

    if n < 1 {
        TIMEOUT
    } else if fdset.contains(fd) {
        READY
    } else {
        INTERRUPTED
    }
}

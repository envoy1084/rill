//! What running a process produced.

use std::time::Duration;

use bytes::Bytes;

/// Raw outcome of running a process, before anything is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawOutcome {
    /// Process exit code, or [`crate::action::TIMEOUT_EXIT_CODE`] on
    /// timeout and `-1` when killed by a signal.
    pub exit_code: i32,
    pub stdout: Bytes,
    pub stderr: Bytes,
    pub duration: Duration,
    /// True when the process was killed because the timeout expired.
    pub timed_out: bool,
}

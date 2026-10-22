//! What running a process produced.

use std::time::Duration;

use bytes::Bytes;

/// Exit code reported when a command exceeds its timeout, as `timeout(1)`
/// does.
pub const TIMEOUT_EXIT_CODE: i32 = 124;

/// Raw outcome of running a process, before anything is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawOutcome {
    /// Process exit code, or [`TIMEOUT_EXIT_CODE`] on timeout and `-1`
    /// when killed by a signal.
    pub exit_code: i32,
    pub stdout: Bytes,
    pub stderr: Bytes,
    pub duration: Duration,
    /// True when the process was killed because the timeout expired.
    pub timed_out: bool,
}

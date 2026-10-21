//! The repository event log, read: every committed state change wrote one
//! event in its own transaction, so a tail of the log sees each change
//! exactly once and in commit order.
//!
//! * [`filter`]: which events a reader wants.
//! * [`tail`]: a resumable stream of the log that waits for new events.
//! * [`retention`]: dropping events older than [`RETENTION`].

pub mod error;
pub mod filter;
pub mod retention;
pub mod tail;

pub use error::{Error, Result};
pub use filter::EventFilter;
pub use retention::{run_retention, sweep, RETENTION};
pub use tail::{tail, Start};

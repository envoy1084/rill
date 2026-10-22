//! Server-streaming watches over the repository event log.
//!
//! Every committed state change wrote one event in its own transaction, so
//! the log is exact: a watch starts at a cursor and follows the log. The
//! watches that report store state (trunk, the version log) use the event
//! counter only as a wake-up and read what they send from the store.

mod events;
mod history;
mod refs;
mod stream;

pub(crate) use stream::forward;
pub use stream::EventStream;

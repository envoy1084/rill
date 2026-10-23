//! Talking to the daemon from the CLI and other tools.

mod connect;
mod convert;
mod spawn;

pub use connect::DaemonClient;
pub use convert::local_changes;
pub use spawn::connect_or_start;

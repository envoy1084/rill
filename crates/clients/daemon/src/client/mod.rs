//! Talking to the daemon from the CLI and other tools.

mod connect;
mod spawn;

pub use connect::DaemonClient;
pub use spawn::connect_or_start;

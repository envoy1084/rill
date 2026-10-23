//! The daemon's local API, `rill.daemon.v1` over a Unix socket.

pub(crate) mod convert;
mod serve;
mod service;

pub use serve::serve;
pub use service::DaemonSvc;

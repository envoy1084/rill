//! `rill.v1.OperationService`: long-running operations, stored per
//! repository and read back by name.

mod handlers;
mod record;
mod service;

pub use record::{save, started, status_of};
pub use service::OperationSvc;

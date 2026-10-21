//! Shared server state: stores, configuration, authentication and the
//! per-repository service graphs.

mod app;
mod lifecycle;
pub mod policy;
mod registry;
mod stores;

pub use app::AppState;
pub use lifecycle::Lifecycle;
pub use registry::{RepoContext, RepoRegistry};
pub use stores::open_stores;

//! Shared server state: stores, configuration, authentication and the
//! per-repository service graphs.

mod app;
pub mod decisions;
mod grep;
mod index_requests;
mod lifecycle;
pub mod policy;
mod registry;
mod releases;
mod repo_build;
mod repo_context;
mod stores;

pub use app::AppState;
pub use grep::GrepOutcome;
pub use lifecycle::Lifecycle;
pub use registry::RepoRegistry;
pub use releases::ReleaseLanders;
pub use repo_context::RepoContext;
pub use stores::open_stores;

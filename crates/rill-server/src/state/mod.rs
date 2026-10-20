//! Shared server state: stores, configuration, authentication and the
//! per-repository service graphs.

mod app;
mod events;
pub mod policy;
mod registry;

pub use app::{open_stores, AppState};
pub use events::{land_to_bus, EventBus, RepoEvent, EVENT_CAPACITY};
pub use registry::{RepoContext, RepoRegistry, TicketInfo};

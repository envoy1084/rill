//! The gRPC services. Handlers are thin: decode, check permission, call
//! one lower crate, encode.

pub mod admin;
pub mod common;
pub mod diff;
pub mod intent;
pub mod lander;
pub mod repo;
pub mod review;
pub mod workspace;

pub use admin::AdminSvc;
pub use intent::IntentSvc;
pub use lander::LanderSvc;
pub use repo::RepoSvc;
pub use review::ReviewSvc;
pub use workspace::WorkspaceSvc;

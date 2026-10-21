//! The gRPC services. Handlers are thin: decode, check permission, call
//! one lower crate, encode.

pub mod admin;
pub mod budget;
pub mod common;
pub mod diff;
pub mod idem;
pub mod identity;
pub mod intent;
pub mod lander;
pub mod orgs;
pub mod repo;
pub mod review;
pub mod session;
pub mod workspace;

pub use admin::AdminSvc;
pub use identity::IdentitySvc;
pub use intent::IntentSvc;
pub use lander::LanderSvc;
pub use repo::RepoSvc;
pub use review::ReviewSvc;
pub use session::SessionSvc;
pub use workspace::WorkspaceSvc;

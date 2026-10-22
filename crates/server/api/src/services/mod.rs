//! The gRPC services. Handlers are thin: decode, check permission, call
//! one lower crate, encode.

pub mod access;
pub mod admin;
pub mod audit;
pub mod budget;
mod call;
mod checks;
pub mod common;
pub mod diff;
pub mod exec;
pub mod idem;
pub mod identity;
pub mod intent;
pub mod lander;
pub mod orgs;
pub mod protection;
pub mod repo;
pub mod review;
mod secrets;
pub mod session;
mod trunk_health;
pub mod workspace;

pub use access::AccessSvc;
pub use admin::AdminSvc;
pub use audit::AuditSvc;
pub use call::Call;
pub use checks::ChecksSvc;
pub use exec::ExecSvc;
pub use identity::IdentitySvc;
pub use intent::IntentSvc;
pub use lander::LanderSvc;
pub use protection::ProtectionSvc;
pub use repo::RepoSvc;
pub use review::ReviewSvc;
pub use secrets::SecretSvc;
pub use session::SessionSvc;
pub use workspace::WorkspaceSvc;

//! `rill.v1.AdminService`: identity, capability tokens and policies.
//!
//! Admin requests carry no `repo` field; the repository is named inside
//! the scope or policy name instead:
//!
//! * capability scopes are `repo:<repo>` or `repo:<repo>/<dir>[/*]`;
//! * policy names are `<repo>/submit` (Cedar) or `<repo>/risk` (JSON).

mod health;
mod policies;
mod scopes;
mod service;
mod tokens;
mod trunk;

pub use health::health_components;
pub use service::AdminSvc;

//! Authentication (who is calling) and authorization (what they may do).

mod authenticator;
mod context;
pub mod http;
mod instance;
mod layer;
mod permit;
mod rate;

pub use authenticator::{authenticate, static_tokens};
pub use context::{bearer_token, caller, dev_context, dev_principal, DEV_PRINCIPAL};
pub use instance::{administers_org, require_instance_admin, require_org_admin};
pub use layer::{AuthLayer, AuthService};
pub use permit::Permit;
pub use rate::{RateLimits, SWEEP_EVERY};

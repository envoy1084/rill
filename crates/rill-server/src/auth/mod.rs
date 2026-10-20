//! Authentication (who is calling) and authorization (what they may do).

mod authenticator;
mod context;
pub mod http;
mod layer;
mod permit;

pub use authenticator::{authenticate, build_authenticator, build_issuer};
pub use context::{bearer_token, caller, dev_context, dev_principal, DEV_PRINCIPAL};
pub use layer::{AuthLayer, AuthService};
pub use permit::Permit;

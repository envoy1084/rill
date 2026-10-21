//! Identity for Rill.
//!
//! * [`keys`]: token signing keys, generated here, wrapped by a key
//!   service (a local master key, Vault Transit, AWS KMS or Google Cloud
//!   KMS), stored in the metadata store and rotated on a schedule.
//! * [`oidc`]: provider discovery, JWT verification against an issuer's
//!   keys, and the authorization-code flow with PKCE.
//! * [`saml`] (feature `saml`): SAML 2.0 requests and verified responses.

pub mod agents;
pub mod error;
pub mod keys;
pub mod oidc;
#[cfg(feature = "saml")]
pub mod saml;
pub mod scim;
pub mod signin;
#[cfg(feature = "testing")]
pub mod testing;

pub use error::{Error, Result};

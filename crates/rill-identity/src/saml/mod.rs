//! SAML 2.0 identity providers: the AuthnRequest that starts a sign-in and
//! the signed response that ends it, read as the same claims an ID token
//! gives.

mod claims;
mod provider;

pub use claims::assertion_claims;
pub use provider::{idp_entity_id, SamlProvider, SamlRequest};

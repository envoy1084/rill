//! Signing in over HTTP: the browser flow through an organization's
//! identity provider, the OAuth token endpoint that refreshes sessions and
//! exchanges ID tokens (the CLI's device flow ends here), and the provider
//! list clients start from. With the `saml` feature, SAML 2.0 providers
//! sign browsers in too. These routes are open; each checks what it is
//! given.

mod browser;
pub mod flows;
mod identity_provider;
mod oauth;
mod page;
mod providers;
mod router;
#[cfg(feature = "saml")]
mod saml;
mod settings;

use identity_provider::{client_secret, provider};
pub use router::router;
pub(crate) use settings::public_url;
use settings::{redirect_uri, session_ttl_ms};

//! OpenID Connect: browser sign-in through `openidconnect`, and verifying
//! workload JWTs from any OIDC issuer (discovery, JWKS, `jsonwebtoken`).

mod browser;
mod discovery;
mod jwks;
mod jwt;
mod registry;

pub use browser::{begin, finish, AuthorizationRequest, Registration};
pub use discovery::{discover, ProviderMetadata};
pub use jwks::{JwksCache, JwksSource};
pub use jwt::{unverified_issuer, Claims, JwtVerifier};
pub use registry::Issuers;

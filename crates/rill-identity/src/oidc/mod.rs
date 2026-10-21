//! OpenID Connect: discovering providers, verifying the JWTs they sign
//! (ID tokens, and workload tokens from any OIDC issuer), and the
//! authorization-code flow with PKCE.

mod code_flow;
mod discovery;
mod jwks;
mod jwt;
mod registry;

pub use code_flow::{authorize_url, exchange_code, AuthorizationRequest, CodeExchange};
pub use discovery::{discover, ProviderMetadata};
pub use jwks::{JwksCache, JwksSource};
pub use jwt::{unverified_issuer, Claims, JwtVerifier};
pub use registry::Issuers;

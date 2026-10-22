//! The authenticated caller, as stored in request extensions.

use rill_policy::AuthContext;
use rill_types::Principal;

use crate::services::Call;

use rill_runtime::error::{Error, Result};

/// Name of the principal anonymous requests act as in dev mode.
pub const DEV_PRINCIPAL: &str = "dev";

/// The `dev` human principal.
pub fn dev_principal() -> Principal {
    Principal::human(DEV_PRINCIPAL)
}

/// An auth context for the `dev` principal with no capability bound.
pub fn dev_context() -> AuthContext {
    AuthContext {
        principal: dev_principal(),
        teams: Vec::new(),
        capability: None,
        claims: None,
        scope: None,
        relations_seen: Vec::new(),
    }
}

/// The header naming relation versions a client has observed:
/// `rp_<hex>=<version>` pairs separated by commas.
pub const RELATIONS_SEEN_HEADER: &str = "x-rill-relations";

/// Parse [`RELATIONS_SEEN_HEADER`], skipping malformed pairs.
pub fn relations_seen(header: Option<&str>) -> Vec<(rill_types::RepoId, i64)> {
    header
        .into_iter()
        .flat_map(|h| h.split(','))
        .filter_map(|pair| {
            let (repo, version) = pair.trim().split_once('=')?;
            Some((repo.parse().ok()?, version.parse().ok()?))
        })
        .collect()
}

/// A request the auth middleware can attach a caller to: a Rill API call
/// or a REAPI (tonic) request.
pub trait Extended {
    /// The request's extensions.
    fn extensions(&self) -> &http::Extensions;
}

impl<T> Extended for Call<T> {
    fn extensions(&self) -> &http::Extensions {
        Call::extensions(self)
    }
}

impl<T> Extended for tonic::Request<T> {
    fn extensions(&self) -> &http::Extensions {
        tonic::Request::extensions(self)
    }
}

/// The caller the auth middleware attached to a request. Missing means the
/// middleware did not run, which is a server wiring bug, not a client
/// error; it is still reported as `UNAUTHENTICATED` so nothing proceeds.
pub fn caller(request: &impl Extended) -> Result<AuthContext> {
    request
        .extensions()
        .get::<AuthContext>()
        .cloned()
        .ok_or_else(|| Error::Unauthenticated("request carries no caller".into()))
}

/// Strip `Bearer ` from an `Authorization` header value.
pub fn bearer_token(header: &str) -> Option<&str> {
    let (scheme, token) = header.trim().split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") {
        let token = token.trim();
        (!token.is_empty()).then_some(token)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bearer() {
        assert_eq!(bearer_token("Bearer abc"), Some("abc"));
        assert_eq!(bearer_token("bearer  abc "), Some("abc"));
        assert_eq!(bearer_token("Basic abc"), None);
        assert_eq!(bearer_token("Bearer "), None);
    }

    #[test]
    fn missing_caller_is_unauthenticated() {
        let ctx = connectrpc::RequestContext::new(http::HeaderMap::new());
        let req = Call::with_message(ctx, ());
        assert!(matches!(caller(&req), Err(Error::Unauthenticated(_))));
        let mut extensions = http::Extensions::new();
        extensions.insert(dev_context());
        let ctx =
            connectrpc::RequestContext::new(http::HeaderMap::new()).with_extensions(extensions);
        let req = Call::with_message(ctx, ());
        assert_eq!(caller(&req).unwrap().principal, dev_principal());
    }
}

//! The authenticated caller, as stored in request extensions.

use rill_policy::AuthContext;
use rill_types::Principal;
use tonic::Request;

use crate::error::{Error, Result};

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
    }
}

/// The caller the auth middleware attached to a request. Missing means the
/// middleware did not run, which is a server wiring bug, not a client
/// error; it is still reported as `UNAUTHENTICATED` so nothing proceeds.
pub fn caller<T>(request: &Request<T>) -> Result<AuthContext> {
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
        let req = Request::new(());
        assert!(matches!(caller(&req), Err(Error::Unauthenticated(_))));
        let mut req = Request::new(());
        req.extensions_mut().insert(dev_context());
        assert_eq!(caller(&req).unwrap().principal, dev_principal());
    }
}

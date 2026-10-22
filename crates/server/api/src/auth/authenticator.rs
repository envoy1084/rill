//! Resolving the caller from a bearer token.
//!
//! Every token the instance signed is verified against this server's
//! identity view, which also refuses revoked tokens and sessions. A
//! dev-mode server may additionally accept static tokens from its
//! configuration.

use rill_policy::{AuthContext, AuthHeaders, Authenticator};

use super::context::{bearer_token, dev_context};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// Resolve the caller for a request from its `Authorization` header.
///
/// Without a header: the `dev` principal in dev mode, otherwise
/// `UNAUTHENTICATED`. With a header: the token must authenticate even in
/// dev mode, so tests and tools can act as specific principals.
pub async fn authenticate(state: &AppState, authorization: Option<&str>) -> Result<AuthContext> {
    let Some(token) = authorization.and_then(bearer_token) else {
        return if state.dev_mode() {
            Ok(dev_context())
        } else {
            Err(Error::Unauthenticated(
                "missing bearer token in authorization header".into(),
            ))
        };
    };
    if let Some(table) = &state.static_tokens {
        if let Ok(ctx) = table.authenticate(&AuthHeaders::bearer(token)).await {
            return Ok(ctx);
        }
    }
    let verified = state.identity.verify(token).await?;
    state.identity.context(verified).await
}

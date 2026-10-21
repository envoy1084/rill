//! Resolving the caller from a bearer token.
//!
//! Every token the instance signed is verified against this server's
//! identity view, which also refuses revoked tokens and sessions. A
//! dev-mode server may additionally accept static tokens from its
//! configuration.

use rill_policy::{AuthContext, AuthHeaders, Authenticator, StaticTokenAuthenticator};
use rill_types::{Principal, PrincipalKind};

use super::context::{bearer_token, dev_context};
use crate::config::ServerConfig;
use crate::error::{Error, Result};
use crate::state::AppState;

/// The static token table, when the configuration has one. Static tokens
/// are refused outside dev mode.
pub fn static_tokens(config: &ServerConfig) -> Result<Option<StaticTokenAuthenticator>> {
    if config.auth.tokens.is_empty() {
        return Ok(None);
    }
    if !config.dev_mode {
        return Err(Error::Config(
            "static tokens are only accepted in dev mode; issue tokens instead".into(),
        ));
    }
    let mut table = StaticTokenAuthenticator::new();
    for t in &config.auth.tokens {
        let principal = match t.kind {
            PrincipalKind::Human => Principal::human(&t.principal_name),
            PrincipalKind::Agent => Principal::agent(&t.principal_name),
            PrincipalKind::Service => Principal::service(&t.principal_name),
        };
        table.insert(t.token.clone(), principal, t.teams.clone());
    }
    Ok(Some(table))
}

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
    Ok(AuthContext {
        principal: verified.principal,
        teams: Vec::new(),
        capability: verified.capability,
        claims: Some(verified.claims),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StaticToken;

    #[test]
    fn static_tokens_need_dev_mode() {
        let mut config = ServerConfig::default();
        config.auth.tokens.push(StaticToken {
            token: "t".into(),
            principal_name: "alice".into(),
            kind: PrincipalKind::Human,
            teams: vec![],
        });
        assert!(matches!(static_tokens(&config), Err(Error::Config(_))));
        config.dev_mode = true;
        assert!(static_tokens(&config).unwrap().is_some());
    }
}

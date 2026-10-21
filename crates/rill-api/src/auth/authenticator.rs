//! Building the token authenticator and issuer from configuration.
//!
//! Whatever the configured scheme, the server also accepts Biscuit tokens
//! minted by its own issuer so `IssueToken` works everywhere; a composite
//! authenticator tries the static table first and then each Biscuit root.

use std::sync::Arc;

use async_trait::async_trait;
use rill_policy::{
    AuthContext, AuthHeaders, Authenticator, BiscuitAuthenticator, Issuer, KeyRing, SigningKey,
    StaticTokenAuthenticator, TokenLifetimes, Verifier,
};
use rill_types::{Principal, PrincipalKind};

use super::context::{bearer_token, dev_context};
use crate::config::AuthConfig;
use crate::error::{Error, Result};
use crate::state::AppState;

/// The token issuer: the configured Biscuit private key, or a key
/// generated for this process (tokens then die with the process).
pub fn build_issuer(config: &AuthConfig) -> Result<Issuer> {
    match config {
        AuthConfig::Biscuit {
            root_private_key_hex: Some(private),
            ..
        } => {
            let bytes = hex::decode(private)
                .map_err(|e| Error::Config(format!("root_private_key_hex: {e}")))?;
            Ok(Issuer::new(
                SigningKey::from_secret_bytes(0, &bytes)?,
                TokenLifetimes::default(),
            ))
        }
        _ => {
            tracing::info!("no issuing key configured; using a process-local key");
            Ok(Issuer::generate())
        }
    }
}

/// Authenticates against the static token table and every Biscuit root.
struct CompositeAuthenticator {
    static_tokens: Option<StaticTokenAuthenticator>,
    biscuit: Vec<BiscuitAuthenticator>,
}

#[async_trait]
impl Authenticator for CompositeAuthenticator {
    async fn authenticate(&self, headers: &AuthHeaders) -> rill_policy::Result<AuthContext> {
        if let Some(static_tokens) = &self.static_tokens {
            if let Ok(ctx) = static_tokens.authenticate(headers).await {
                return Ok(ctx);
            }
        }
        let mut last = rill_policy::Error::Unauthenticated("missing bearer token".into());
        for verifier in &self.biscuit {
            match verifier.authenticate(headers).await {
                Ok(ctx) => return Ok(ctx),
                Err(e @ rill_policy::Error::TokenExpired { .. }) => return Err(e),
                Err(e) => last = e,
            }
        }
        if self.static_tokens.is_some() {
            return Err(rill_policy::Error::Unauthenticated(
                "unknown or invalid bearer token".into(),
            ));
        }
        Err(last)
    }
}

/// Build the authenticator for `config`, also trusting tokens from `issuer`.
pub fn build_authenticator(config: &AuthConfig, issuer: &Issuer) -> Result<Arc<dyn Authenticator>> {
    let own = BiscuitAuthenticator::new(Verifier::new(issuer.key_ring()));
    let (static_tokens, mut biscuit) = match config {
        AuthConfig::Static { tokens } => {
            let mut table = StaticTokenAuthenticator::new();
            for t in tokens {
                let principal = match t.kind {
                    PrincipalKind::Human => Principal::human(&t.principal_name),
                    PrincipalKind::Agent => Principal::agent(&t.principal_name),
                    PrincipalKind::Service => Principal::service(&t.principal_name),
                };
                table.insert(t.token.clone(), principal, t.teams.clone());
            }
            (Some(table), Vec::new())
        }
        AuthConfig::Biscuit {
            root_public_key_hex,
            ..
        } => {
            let bytes = hex::decode(root_public_key_hex)
                .map_err(|e| Error::Config(format!("root_public_key_hex: {e}")))?;
            let mut ring = KeyRing::new();
            ring.insert_bytes(0, &bytes, None)?;
            let verifier = Verifier::new(ring);
            let same_as_issuer = bytes == issuer.public_key().to_bytes();
            if same_as_issuer {
                (None, Vec::new())
            } else {
                (None, vec![BiscuitAuthenticator::new(verifier)])
            }
        }
    };
    biscuit.push(own);
    Ok(Arc::new(CompositeAuthenticator {
        static_tokens,
        biscuit,
    }))
}

/// Resolve the caller for a request from its `Authorization` header.
///
/// Without a header: the `dev` principal in dev mode, otherwise
/// `UNAUTHENTICATED`. With a header: the token must authenticate even in
/// dev mode, so tests and tools can act as specific principals.
pub async fn authenticate(state: &AppState, authorization: Option<&str>) -> Result<AuthContext> {
    let token = authorization.and_then(bearer_token);
    match token {
        None if state.dev_mode() => Ok(dev_context()),
        None => Err(Error::Unauthenticated(
            "missing bearer token in authorization header".into(),
        )),
        Some(token) => Ok(state.auth.authenticate(&AuthHeaders::bearer(token)).await?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StaticToken;
    use rill_policy::{Capability, Verb};
    use rill_types::{RepoId, Timestamp};

    #[tokio::test]
    async fn static_and_issued_tokens_both_work() {
        let config = AuthConfig::Static {
            tokens: vec![StaticToken {
                token: "t".into(),
                principal_name: "alice".into(),
                kind: PrincipalKind::Human,
                teams: vec!["core".into()],
            }],
        };
        let issuer = build_issuer(&config).unwrap();
        let auth = build_authenticator(&config, &issuer).unwrap();
        let ctx = auth.authenticate(&AuthHeaders::bearer("t")).await.unwrap();
        assert_eq!(ctx.principal.name, "alice");
        assert_eq!(ctx.teams, vec!["core"]);
        assert!(auth
            .authenticate(&AuthHeaders::bearer("nope"))
            .await
            .is_err());

        let cap = Capability::repo_wide(
            RepoId::new(),
            [Verb::Read],
            Timestamp::from_millis(Timestamp::now().millis() + 60_000),
        );
        let claims = rill_policy::TokenClaims::new("default", Timestamp::now(), 60_000);
        let token = issuer
            .issue(&Principal::agent("bot"), &claims, Some(&cap))
            .unwrap();
        let ctx = auth
            .authenticate(&AuthHeaders::bearer(&token))
            .await
            .unwrap();
        assert_eq!(ctx.principal.name, "bot");
        assert_eq!(ctx.capability, Some(cap));
    }
}

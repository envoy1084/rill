//! Revoking tokens and sessions.

use rill_proto::rill::v1 as pb;
use rill_types::identity::{IdentityChange, Revocation};
use rill_types::{SessionId, Timestamp, TokenId};

use crate::auth::{caller, require_instance_admin};
use crate::services::Call;
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

/// Revoke the presented token (anyone holding it may), or a token id
/// (administrators).
pub async fn token(
    state: &AppState,
    request: Call<pb::RevokeTokenRequest>,
) -> Result<pb::RevokeTokenResponse> {
    let ctx = caller(&request)?;
    let req = request.into_inner();
    let now = Timestamp::now();
    let (token_id, expires_at) = match req.target {
        Some(pb::revoke_token_request::Target::Token(token)) => {
            let verified = state.identity.verify(&token).await?;
            (verified.claims.token_id, verified.claims.expires_at)
        }
        Some(pb::revoke_token_request::Target::TokenId(id)) => {
            require_instance_admin(state, &ctx)?;
            let id: TokenId = id
                .parse()
                .map_err(|e| Error::invalid(format!("token_id: {e}")))?;
            // The token's own expiry is unknown here; outlive every token.
            let longest = state
                .identity
                .lifetimes()
                .max_for(rill_types::PrincipalKind::Service);
            (id, Timestamp::from_millis(now.millis() + longest))
        }
        None => return Err(Error::invalid("name a token or a token_id")),
    };
    let revocation = Revocation {
        token_id,
        revoked_by: ctx.principal.clone(),
        revoked_at: now,
        reason: req.reason,
        expires_at,
    };
    state
        .identity
        .store()
        .revoke_token(&revocation, &ctx.principal)
        .await?;
    // Refused here at once; other servers follow the identity log.
    state.identity.apply(&IdentityChange::TokenRevoked {
        token_id,
        expires_at,
    });
    tracing::info!(by = %ctx.principal, token = %token_id, "token revoked");
    Ok(pb::RevokeTokenResponse {
        token_id: token_id.to_string(),
    })
}

/// End a session: the caller's own, or any for administrators.
pub async fn session(
    state: &AppState,
    request: Call<pb::RevokeSessionRequest>,
) -> Result<pb::RevokeSessionResponse> {
    let ctx = caller(&request)?;
    let id: SessionId = request
        .get_ref()
        .session
        .parse()
        .map_err(|e| Error::invalid(format!("session: {e}")))?;
    let store = state.identity.store();
    let session = store
        .session(&id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("session {id}")))?;
    if session.principal.id != ctx.principal.id {
        require_instance_admin(state, &ctx)?;
    }
    store.revoke_session(&id, &ctx.principal).await?;
    state
        .identity
        .apply(&IdentityChange::SessionRevoked { session: id });
    tracing::info!(by = %ctx.principal, session = %id, "session revoked");
    Ok(pb::RevokeSessionResponse {})
}

//! Token signing keys, revocation and the kill switch.

use rill_proto::rill::v1 as pb;

use super::IdentityClient;
use crate::client::retry_idempotent;
use crate::error::Result;

impl IdentityClient {
    /// The token signing keys, active and retired (instance
    /// administrators).
    pub async fn token_keys(&self) -> Result<Vec<pb::TokenKey>> {
        let resp = retry_idempotent(|| {
            let svc = self.svc();
            async move { svc.list_token_keys(pb::ListTokenKeysRequest {}).await }
        })
        .await?
        .into_owned();
        Ok(resp.keys)
    }

    /// Store a new signing key, active after the activation delay
    /// (instance administrators).
    pub async fn rotate_token_keys(&self) -> Result<pb::RotateTokenKeysResponse> {
        let request = pb::RotateTokenKeysRequest {
            request_id: Self::request_id(),
        };
        Ok(retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.rotate_token_keys(request).await }
        })
        .await?
        .into_owned())
    }

    /// Revoke a token the caller presents, or (instance administrators) a
    /// token id; the revoked token id.
    pub async fn revoke_token(
        &self,
        target: pb::revoke_token_request::Target,
        reason: &str,
    ) -> Result<String> {
        let request = pb::RevokeTokenRequest {
            request_id: Self::request_id(),
            target: Some(target),
            reason: reason.to_string(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.revoke_token(request).await }
        })
        .await?
        .into_owned();
        Ok(resp.token_id)
    }

    /// End session `session` (`ss_…`); every token issued under it is
    /// refused from then on.
    pub async fn revoke_session(&self, session: &str) -> Result<()> {
        let request = pb::RevokeSessionRequest {
            request_id: Self::request_id(),
            session: session.to_string(),
        };
        retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.revoke_session(request).await }
        })
        .await?;
        Ok(())
    }

    /// Stop an agent, a session, a sponsor's agents or a model across the
    /// org at once.
    pub async fn kill_switch(
        &self,
        target: pb::kill_switch_request::Target,
        reason: &str,
    ) -> Result<pb::KillSwitchResponse> {
        let request = pb::KillSwitchRequest {
            request_id: Self::request_id(),
            parent: self.org.clone(),
            target: Some(target),
            reason: reason.to_string(),
        };
        Ok(retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.kill_switch(request).await }
        })
        .await?
        .into_owned())
    }
}

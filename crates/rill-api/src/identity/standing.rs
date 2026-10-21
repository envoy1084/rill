//! Whether the authors of a change may still have it land: the lander asks
//! before every landing, so a killed agent's queued work is ejected and
//! its open changes cannot land.

use async_trait::async_trait;
use rill_types::{Principal, PrincipalKind, Provenance, SessionId, Timestamp};

use super::view::IdentityView;

/// Judges the principals behind a change at landing time.
#[async_trait]
pub trait AuthorStanding: Send + Sync {
    /// Why a version by `author` (with `provenance`), requested by
    /// `requester` in `org`, must not land now; `None` when it may.
    async fn refusal(
        &self,
        org: &str,
        author: &Principal,
        provenance: &Provenance,
        requester: &Principal,
    ) -> Option<String>;
}

#[async_trait]
impl AuthorStanding for IdentityView {
    async fn refusal(
        &self,
        org: &str,
        author: &Principal,
        provenance: &Provenance,
        requester: &Principal,
    ) -> Option<String> {
        let session = provenance
            .session
            .as_deref()
            .and_then(|s| s.parse::<SessionId>().ok());
        if let Some(session) = session {
            if self.session_revoked(&session) {
                return Some(format!("the session that wrote it ({session}) was revoked"));
            }
        }
        match self
            .principal_refusal(org, author, provenance.model.as_deref(), true)
            .await
        {
            Some(reason) => Some(format!("its author cannot land: {reason}")),
            None if requester != author => self
                .principal_refusal(org, requester, None, false)
                .await
                .map(|reason| format!("its requester cannot land: {reason}")),
            None => None,
        }
    }
}

impl IdentityView {
    /// Why `principal` may not act, checking an agent's model only when
    /// `with_model`. Lookup failures refuse: landing fails closed.
    async fn principal_refusal(
        &self,
        org: &str,
        principal: &Principal,
        model: Option<&str>,
        with_model: bool,
    ) -> Option<String> {
        match principal.kind {
            PrincipalKind::Agent => match self.agent(org, &principal.name).await {
                Ok(Some(agent)) => {
                    let check = if with_model {
                        self.check_agent(&agent, model)
                            .await
                            .map_err(|e| e.to_string())
                    } else {
                        rill_identity::agents::registration_holds(
                            self.store(),
                            &agent,
                            Timestamp::now(),
                        )
                        .await
                        .map_err(|e| e.to_string())
                    };
                    check.err()
                }
                Ok(None) => None,
                Err(e) => Some(format!("identity unavailable: {e}")),
            },
            PrincipalKind::Human => match self.user(org, &principal.name).await {
                Ok(Some(user)) if user.principal.kind == principal.kind && !user.is_active() => {
                    Some(format!("{principal} is suspended"))
                }
                Ok(_) => None,
                Err(e) => Some(format!("identity unavailable: {e}")),
            },
            PrincipalKind::Service => None,
        }
    }
}

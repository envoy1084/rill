//! The kill switch: stop an agent, a session, a sponsor's agents or a
//! model across the org, at once.

use rill_proto::rill::v1 as pb;
use rill_types::identity::{AccountStatus, AgentRecord, IdentityChange, Sponsor};
use rill_types::{Principal, SessionId, Timestamp};
use tonic::Request;

use crate::auth::{caller, require_org_admin};
use crate::error::{Error, Result};
use crate::state::AppState;

/// Sessions a sweep revokes at most.
const SESSION_SCAN: usize = 100_000;

pub async fn kill(
    state: &AppState,
    request: Request<pb::KillSwitchRequest>,
) -> Result<pb::KillSwitchResponse> {
    let ctx = caller(&request)?;
    let req = request.into_inner();
    require_org_admin(state, &ctx, &req.org).await?;
    let store = state.identity.store();
    let actor = &ctx.principal;
    let mut out = pb::KillSwitchResponse::default();
    use pb::kill_switch_request::Target;
    match req.target.ok_or_else(|| Error::invalid("name a target"))? {
        Target::Agent(name) => {
            let agent = store
                .agent(&req.org, &name)
                .await?
                .ok_or_else(|| Error::NotFound(format!("agent {}/{name}", req.org)))?;
            suspend(state, &agent, actor, &mut out).await?;
        }
        Target::Sponsor(label) => {
            let sponsor = match label.split_once(':') {
                Some(("user", n)) => Sponsor::User(n.into()),
                Some(("team", n)) => Sponsor::Team(n.into()),
                _ => return Err(Error::invalid("sponsor: use user:<name> or team:<name>")),
            };
            for agent in store.agents(&req.org).await? {
                if agent.sponsor == sponsor {
                    suspend(state, &agent, actor, &mut out).await?;
                }
            }
        }
        Target::Session(id) => {
            let id: SessionId = id
                .parse()
                .map_err(|e| Error::invalid(format!("session: {e}")))?;
            let session = store
                .session(&id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("session {id}")))?;
            if session.org != req.org {
                return Err(Error::denied("the session belongs to another org"));
            }
            revoke(state, &id, actor).await?;
            out.sessions_revoked.push(id.to_string());
        }
        Target::Model(model) => {
            let mut settings = store.org_settings(&req.org).await?;
            if !settings.blocked_models.contains(&model) {
                settings.blocked_models.push(model.clone());
                store.put_org_settings(&settings, actor).await?;
            }
            state.identity.apply(&IdentityChange::OrgSettingsChanged {
                org: req.org.clone(),
            });
            for session in store.live_sessions(Timestamp::now(), SESSION_SCAN).await? {
                if session.org == req.org && session.model.as_deref() == Some(model.as_str()) {
                    revoke(state, &session.id, actor).await?;
                    out.sessions_revoked.push(session.id.to_string());
                }
            }
            out.models_blocked.push(model);
        }
    }
    tracing::warn!(by = %actor, org = %req.org, reason = %req.reason, agents = ?out.agents_suspended, sessions = out.sessions_revoked.len(), models = ?out.models_blocked, "kill switch");
    Ok(out)
}

async fn revoke(state: &AppState, id: &SessionId, actor: &Principal) -> Result<()> {
    state.identity.store().revoke_session(id, actor).await?;
    state
        .identity
        .apply(&IdentityChange::SessionRevoked { session: *id });
    Ok(())
}

/// Suspend `agent` and revoke its live sessions.
async fn suspend(
    state: &AppState,
    agent: &AgentRecord,
    actor: &Principal,
    out: &mut pb::KillSwitchResponse,
) -> Result<()> {
    let store = state.identity.store();
    if agent.status != AccountStatus::Suspended {
        let after = AgentRecord {
            status: AccountStatus::Suspended,
            updated_at: Timestamp::now(),
            ..agent.clone()
        };
        store.update_agent(agent, &after, actor).await?;
        state.identity.apply(&IdentityChange::AgentChanged {
            org: agent.org.clone(),
            name: agent.principal.name.clone(),
        });
    }
    out.agents_suspended.push(agent.principal.name.clone());
    for session in store.live_sessions(Timestamp::now(), SESSION_SCAN).await? {
        if session.org == agent.org && session.principal == agent.principal {
            revoke(state, &session.id, actor).await?;
            out.sessions_revoked.push(session.id.to_string());
        }
    }
    Ok(())
}

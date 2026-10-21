//! Whether an agent may act: its registration, its model and its sponsor.

use rill_store::identity::IdentityStore;
use rill_types::identity::{AgentRecord, OrgIdentitySettings, Sponsor};
use rill_types::Timestamp;

use crate::error::{Error, Result};

/// Whether `sponsor` of `org` is an active user, or a team some active user
/// of `org` belongs to.
pub async fn sponsor_is_active(
    store: &IdentityStore,
    org: &str,
    sponsor: &Sponsor,
) -> Result<bool> {
    Ok(match sponsor {
        Sponsor::User(name) => store.user(org, name).await?.is_some_and(|u| u.is_active()),
        Sponsor::Team(team) => store
            .users(org)
            .await?
            .iter()
            .any(|u| u.is_active() && u.teams.iter().any(|t| t == team)),
    })
}

/// Refuse unless `agent` may act at `now` as `model`: registered and
/// unexpired, the model allowed by the agent and not blocked by its org,
/// and its sponsor active.
pub async fn agent_may_act(
    store: &IdentityStore,
    agent: &AgentRecord,
    model: Option<&str>,
    settings: &OrgIdentitySettings,
    now: Timestamp,
) -> Result<()> {
    if !agent.is_live_at(now) {
        return Err(Error::Denied(format!(
            "agent {} is suspended or expired",
            agent.principal.name
        )));
    }
    if !agent.allows_model(model) {
        return Err(Error::Denied(format!(
            "agent {} may not run as model {model:?}",
            agent.principal.name
        )));
    }
    if let Some(m) = model {
        if settings.blocked_models.iter().any(|b| b == m) {
            return Err(Error::Denied(format!(
                "model {m:?} is blocked in {}",
                agent.org
            )));
        }
    }
    if !sponsor_is_active(store, &agent.org, &agent.sponsor).await? {
        return Err(Error::Denied(format!(
            "agent {}'s sponsor {} is not active",
            agent.principal.name,
            agent.sponsor.label()
        )));
    }
    Ok(())
}

/// Refuse unless `agent`'s registration lets it act at `now`, whatever
/// model it runs as: active and unexpired, with an active sponsor. For
/// judging an agent that requested something earlier.
pub async fn registration_holds(
    store: &IdentityStore,
    agent: &AgentRecord,
    now: Timestamp,
) -> Result<()> {
    if !agent.is_live_at(now) {
        return Err(Error::Denied(format!(
            "agent {} is suspended or expired",
            agent.principal.name
        )));
    }
    if !sponsor_is_active(store, &agent.org, &agent.sponsor).await? {
        return Err(Error::Denied(format!(
            "agent {}'s sponsor {} is not active",
            agent.principal.name,
            agent.sponsor.label()
        )));
    }
    Ok(())
}

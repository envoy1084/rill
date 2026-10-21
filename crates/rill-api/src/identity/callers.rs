//! What a verified token's holder may do now: a user's teams, and a
//! registered agent's standing (registration, model, sponsor) and scope.

use std::sync::Arc;

use dashmap::DashMap;
use rill_identity::agents::agent_may_act;
use rill_policy::{AgentScope, AuthContext, Verified};
use rill_types::identity::{AgentRecord, OrgIdentitySettings};
use rill_types::{PrincipalKind, RepoPathBuf, Timestamp};

use super::view::IdentityView;
use crate::error::{Error, Result};

/// Agents, org settings and agent standings, read on first use and dropped
/// when the identity log says they changed.
#[derive(Default)]
pub struct CallerCaches {
    agents: DashMap<(String, String), Option<Arc<AgentRecord>>>,
    settings: DashMap<String, Arc<OrgIdentitySettings>>,
    /// Whether `(org, agent, model)` may act; the refusal when not.
    standing: DashMap<(String, String, Option<String>), Option<String>>,
}

impl CallerCaches {
    pub fn clear(&self) {
        self.agents.clear();
        self.settings.clear();
        self.standing.clear();
    }

    pub fn forget_agent(&self, org: &str, name: &str) {
        self.agents.remove(&(org.to_string(), name.to_string()));
        self.standing.retain(|k, _| !(k.0 == org && k.1 == name));
    }

    pub fn forget_org(&self, org: &str) {
        self.settings.remove(org);
        self.standing.retain(|k, _| k.0 != org);
    }
}

impl IdentityView {
    /// The registered agent `name` of `org`, from the cache.
    pub async fn agent(&self, org: &str, name: &str) -> Result<Option<Arc<AgentRecord>>> {
        let key = (org.to_string(), name.to_string());
        if let Some(cached) = self.callers.agents.get(&key) {
            return Ok(cached.clone());
        }
        let agent = self.store().agent(org, name).await?.map(Arc::new);
        self.callers.agents.insert(key, agent.clone());
        Ok(agent)
    }

    /// `org`'s identity settings, from the cache.
    pub async fn org_settings(&self, org: &str) -> Result<Arc<OrgIdentitySettings>> {
        if let Some(cached) = self.callers.settings.get(org) {
            return Ok(cached.clone());
        }
        let settings = Arc::new(self.store().org_settings(org).await?);
        self.callers
            .settings
            .insert(org.to_string(), settings.clone());
        Ok(settings)
    }

    /// Refuse unless registered agent `agent` may act as `model` now.
    pub async fn check_agent(&self, agent: &AgentRecord, model: Option<&str>) -> Result<()> {
        let now = Timestamp::now();
        if !agent.is_live_at(now) {
            return Err(Error::Unauthenticated(format!(
                "agent {} is suspended or expired",
                agent.principal.name
            )));
        }
        let key = (
            agent.org.clone(),
            agent.principal.name.clone(),
            model.map(String::from),
        );
        let refusal = match self.callers.standing.get(&key) {
            Some(cached) => cached.clone(),
            None => {
                let settings = self.org_settings(&agent.org).await?;
                let refusal = match agent_may_act(self.store(), agent, model, &settings, now).await
                {
                    Ok(()) => None,
                    Err(rill_identity::Error::Denied(reason)) => Some(reason),
                    Err(e) => return Err(e.into()),
                };
                self.callers.standing.insert(key, refusal.clone());
                refusal
            }
        };
        match refusal {
            None => Ok(()),
            Some(reason) => Err(Error::Unauthenticated(reason)),
        }
    }

    /// The request context of a verified token: teams for users; standing
    /// and scope for registered agents. A token naming a blocked model is
    /// refused whoever holds it.
    pub async fn context(&self, verified: Verified) -> Result<AuthContext> {
        let org = verified.claims.org.clone();
        let name = verified.principal.name.clone();
        let model = verified.claims.model.clone();
        if let Some(m) = &model {
            if self.org_settings(&org).await?.blocked_models.contains(m) {
                return Err(Error::Unauthenticated(format!(
                    "model {m:?} is blocked in {org}"
                )));
            }
        }
        let mut teams = Vec::new();
        let mut scope = None;
        match verified.principal.kind {
            PrincipalKind::Human => {
                if let Some(user) = self.user(&org, &name).await? {
                    teams = user.teams.clone();
                }
            }
            PrincipalKind::Agent => {
                if let Some(agent) = self.agent(&org, &name).await? {
                    self.check_agent(&agent, model.as_deref()).await?;
                    let paths = agent
                        .paths
                        .iter()
                        .map(|p| RepoPathBuf::new(p.as_str()))
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    scope = Some(AgentScope {
                        org: org.clone(),
                        repos: agent.repos.clone(),
                        paths,
                    });
                }
            }
            PrincipalKind::Service => {}
        }
        Ok(AuthContext {
            principal: verified.principal,
            teams,
            capability: verified.capability,
            claims: Some(verified.claims),
            scope,
        })
    }
}

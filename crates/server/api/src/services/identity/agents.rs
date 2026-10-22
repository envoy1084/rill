//! Registering and managing agents.

use rill_policy::AuthContext;
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::identity::{AccountStatus, AgentBudgets, AgentRecord, Sponsor, WorkloadBinding};
use rill_types::{Principal, RepoPathBuf, Timestamp};
use tonic::Request;

use crate::auth::{caller, require_org_admin};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::AppState;

fn parse_sponsor(text: &str) -> Result<Sponsor> {
    match text.split_once(':') {
        Some(("user", name)) if !name.is_empty() => Ok(Sponsor::User(name.into())),
        Some(("team", name)) if !name.is_empty() => Ok(Sponsor::Team(name.into())),
        _ => Err(Error::invalid(format!(
            "sponsor {text:?}: use user:<name> or team:<name>"
        ))),
    }
}

pub fn agent_proto(a: &AgentRecord) -> pb::Agent {
    pb::Agent {
        spec: Some(pb::AgentSpec {
            org: a.org.clone(),
            name: a.principal.name.clone(),
            sponsor: a.sponsor.label(),
            purpose: a.purpose.clone(),
            repos: a.repos.clone(),
            paths: a.paths.clone(),
            models: a.models.clone(),
            budgets: Some(pb::AgentBudgets {
                versions_per_day: a.budgets.versions_per_day,
                lands_per_day: a.budgets.lands_per_day,
                requests_per_minute: a.budgets.requests_per_minute,
                executions_per_day: a.budgets.executions_per_day,
            }),
            workloads: a
                .workloads
                .iter()
                .map(|w| pb::WorkloadBinding {
                    issuer: w.issuer.clone(),
                    subject: w.subject.clone(),
                    claims: w.claims.clone(),
                })
                .collect(),
            expires_at: a.expires_at.map(timestamp_to_proto),
        }),
        principal: Some(pb::Principal::from(&a.principal)),
        status: match a.status {
            AccountStatus::Active => "active",
            AccountStatus::Suspended => "suspended",
        }
        .into(),
        created_by: Some(pb::Principal::from(&a.created_by)),
        created_at: timestamp_to_proto(a.created_at),
        updated_at: timestamp_to_proto(a.updated_at),
    }
}

/// The registration a spec describes, validated.
fn from_spec(spec: pb::AgentSpec, created_by: &Principal, now: Timestamp) -> Result<AgentRecord> {
    if spec.name.is_empty() || spec.org.is_empty() {
        return Err(Error::invalid("org and name are required"));
    }
    for path in &spec.paths {
        RepoPathBuf::new(path.as_str())
            .map_err(|e| Error::invalid(format!("path {path:?}: {e}")))?;
    }
    if spec.repos.is_empty() {
        return Err(Error::invalid(
            "name the repositories the agent may use, or *",
        ));
    }
    let budgets = spec.budgets.unwrap_or_default();
    Ok(AgentRecord {
        org: spec.org,
        principal: Principal::agent(&spec.name),
        sponsor: parse_sponsor(&spec.sponsor)?,
        purpose: spec.purpose,
        repos: spec.repos,
        paths: spec.paths,
        models: spec.models,
        budgets: AgentBudgets {
            versions_per_day: budgets.versions_per_day,
            lands_per_day: budgets.lands_per_day,
            requests_per_minute: budgets.requests_per_minute,
            executions_per_day: budgets.executions_per_day,
        },
        workloads: spec
            .workloads
            .into_iter()
            .map(|w| WorkloadBinding {
                issuer: w.issuer,
                subject: w.subject,
                claims: w.claims,
            })
            .collect(),
        status: AccountStatus::Active,
        expires_at: spec.expires_at.map(Timestamp::from_millis),
        created_by: created_by.clone(),
        created_at: now,
        updated_at: now,
    })
}

/// Org administrators manage any agent; an active member of the org may
/// manage agents they sponsor themselves.
async fn may_manage(
    state: &AppState,
    ctx: &AuthContext,
    org: &str,
    sponsor: &Sponsor,
) -> Result<()> {
    if require_org_admin(state, ctx, org).await.is_ok() {
        return Ok(());
    }
    let own = matches!(sponsor, Sponsor::User(name) if *name == ctx.principal.name)
        && ctx.capability.is_none()
        && ctx.claims.as_ref().is_some_and(|c| c.org == org);
    if own {
        if let Some(user) = state.identity.user(org, &ctx.principal.name).await? {
            if user.principal.kind == ctx.principal.kind && user.is_active() {
                return Ok(());
            }
        }
    }
    Err(Error::denied(format!(
        "{} may manage only agents they sponsor in {org:?}",
        ctx.principal
    )))
}

pub async fn register(
    state: &AppState,
    request: Request<pb::RegisterAgentRequest>,
) -> Result<pb::RegisterAgentResponse> {
    let ctx = caller(&request)?;
    let spec = request
        .into_inner()
        .agent
        .ok_or_else(|| Error::invalid("agent is required"))?;
    let agent = from_spec(spec, &ctx.principal, Timestamp::now())?;
    may_manage(state, &ctx, &agent.org, &agent.sponsor).await?;
    if state.stores.get_org(&agent.org).await?.is_none() {
        return Err(Error::NotFound(format!("org {:?}", agent.org)));
    }
    state
        .identity
        .store()
        .create_agent(&agent, &ctx.principal)
        .await
        .map_err(|e| match e {
            rill_store::Error::Conflict(m) => Error::AlreadyExists(m),
            other => other.into(),
        })?;
    tracing::info!(by = %ctx.principal, agent = %agent.principal, org = %agent.org, sponsor = %agent.sponsor.label(), "agent registered");
    Ok(pb::RegisterAgentResponse {
        agent: Some(agent_proto(&agent)),
    })
}

pub async fn update(
    state: &AppState,
    request: Request<pb::UpdateAgentRequest>,
) -> Result<pb::UpdateAgentResponse> {
    let ctx = caller(&request)?;
    let req = request.into_inner();
    let store = state.identity.store();
    let before = store
        .agent(&req.org, &req.name)
        .await?
        .ok_or_else(|| Error::NotFound(format!("agent {}/{}", req.org, req.name)))?;
    may_manage(state, &ctx, &before.org, &before.sponsor).await?;
    let now = Timestamp::now();
    let mut after = match req.spec {
        Some(spec) => {
            if spec.org != before.org || spec.name != before.principal.name {
                return Err(Error::invalid("the spec must name the same org and agent"));
            }
            let replaced = from_spec(spec, &before.created_by, before.created_at)?;
            // Moving an agent to another sponsor needs the right to manage
            // agents of that sponsor too.
            may_manage(state, &ctx, &before.org, &replaced.sponsor).await?;
            AgentRecord {
                status: before.status,
                ..replaced
            }
        }
        None => before.clone(),
    };
    if let Some(suspended) = req.suspended {
        after.status = if suspended {
            AccountStatus::Suspended
        } else {
            AccountStatus::Active
        };
    }
    after.updated_at = now;
    store.update_agent(&before, &after, &ctx.principal).await?;
    tracing::info!(by = %ctx.principal, agent = %after.principal, status = ?after.status, "agent updated");
    Ok(pb::UpdateAgentResponse {
        agent: Some(agent_proto(&after)),
    })
}

pub async fn list(
    state: &AppState,
    request: Request<pb::ListAgentsRequest>,
) -> Result<pb::ListAgentsResponse> {
    let ctx = caller(&request)?;
    let org = request.into_inner().org;
    let agents = state.identity.store().agents(&org).await?;
    let admin = require_org_admin(state, &ctx, &org).await.is_ok();
    let visible = agents
        .iter()
        .filter(|a| admin || a.sponsor == Sponsor::User(ctx.principal.name.clone()))
        .map(agent_proto)
        .collect();
    Ok(pb::ListAgentsResponse { agents: visible })
}

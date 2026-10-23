//! Registering and managing agents.

use rill_policy::AuthContext;
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::identity::{AccountStatus, AgentBudgets, AgentRecord, Sponsor, WorkloadBinding};
use rill_types::{Principal, RepoPathBuf, Timestamp};

use crate::auth::{caller, require_org_admin};
use crate::listing::{FieldValue, Filter, Kind, Page, Record};
use crate::services::{etag, names, Call};
use rill_proto::names::AGENT;
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

/// The wire form of an agent, with its name and etag.
pub fn agent_proto(a: &AgentRecord) -> pb::Agent {
    let mut out = agent_fields(a);
    out.etag = etag::of(&out);
    out
}

/// A reviewer profile on the wire.
pub(crate) fn profile_proto(r: &rill_types::identity::ReviewerProfile) -> pb::ReviewerProfile {
    pb::ReviewerProfile {
        model_family: r.model_family.clone(),
        model: r.model.clone(),
        prompt: r.prompt.clone(),
        max_vote: r.max_vote.into(),
        runs_per_day: r.runs_per_day,
    }
}

fn agent_fields(a: &AgentRecord) -> pb::Agent {
    pb::Agent {
        name: names::org_resource(&AGENT, &a.org, &a.principal.name),
        etag: String::new(),
        spec: (pb::AgentSpec {
            org: a.org.clone(),
            name: a.principal.name.clone(),
            sponsor: a.sponsor.label(),
            purpose: a.purpose.clone(),
            repos: a.repos.clone(),
            paths: a.paths.clone(),
            models: a.models.clone(),
            budgets: (pb::AgentBudgets {
                versions_per_day: a.budgets.versions_per_day,
                lands_per_day: a.budgets.lands_per_day,
                requests_per_minute: a.budgets.requests_per_minute,
                executions_per_day: a.budgets.executions_per_day,
            })
            .into(),
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
            reviewer: a.reviewer.as_ref().map(profile_proto).into(),
        })
        .into(),
        principal: pb::Principal::from(&a.principal).into(),
        status: match a.status {
            AccountStatus::Active => "active",
            AccountStatus::Suspended => "suspended",
        }
        .into(),
        created_by: pb::Principal::from(&a.created_by).into(),
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
    let budgets = spec.budgets.into_option().unwrap_or_default();
    let reviewer = match spec.reviewer.into_option() {
        None => None,
        Some(r) => {
            if r.model_family.is_empty() || r.model.is_empty() {
                return Err(Error::invalid(
                    "a reviewer needs a model and a model family",
                ));
            }
            let max_vote = i8::try_from(r.max_vote)
                .ok()
                .filter(|v| (1..=2).contains(v))
                .ok_or_else(|| Error::invalid("a reviewer's max_vote is 1 or 2"))?;
            let prompt = if r.prompt.is_empty() {
                format!(".rill/reviewers/{}/prompt.md", spec.name)
            } else {
                RepoPathBuf::new(&r.prompt)
                    .map_err(|e| Error::invalid(format!("prompt path: {e}")))?
                    .into_string()
            };
            Some(rill_types::identity::ReviewerProfile {
                model_family: r.model_family,
                model: r.model,
                prompt,
                max_vote,
                runs_per_day: r.runs_per_day,
            })
        }
    };
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
        reviewer,
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

pub async fn create(
    state: &AppState,
    request: Call<pb::CreateAgentRequest>,
) -> Result<pb::CreateAgentResponse> {
    let ctx = caller(&request)?;
    let req = request.into_inner();
    let org = names::org(&req.parent)?;
    let spec = req
        .agent
        .into_option()
        .ok_or(rill_proto::ConvertError::missing("agent"))?;
    if spec.org != org {
        return Err(Error::invalid(format!(
            "the agent's org {:?} is not the parent's",
            spec.org
        )));
    }
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
    Ok(pb::CreateAgentResponse {
        agent: agent_proto(&agent).into(),
    })
}

pub async fn get(
    state: &AppState,
    request: Call<pb::GetAgentRequest>,
) -> Result<pb::GetAgentResponse> {
    let ctx = caller(&request)?;
    let (org, name) = names::org_child(&AGENT, &request.get_ref().name)?;
    let agent = state
        .identity
        .store()
        .agent(&org, &name)
        .await?
        .ok_or_else(|| Error::NotFound(format!("agent {org}/{name}")))?;
    may_manage(state, &ctx, &agent.org, &agent.sponsor).await?;
    Ok(pb::GetAgentResponse {
        agent: agent_proto(&agent).into(),
    })
}

pub async fn update(
    state: &AppState,
    request: Call<pb::UpdateAgentRequest>,
) -> Result<pb::UpdateAgentResponse> {
    let ctx = caller(&request)?;
    let req = request.into_inner();
    let given = req
        .agent
        .into_option()
        .ok_or(rill_proto::ConvertError::missing("agent"))?;
    let (org, name) = names::org_child(&AGENT, &given.name)?;
    let mask = req.update_mask.into_option().unwrap_or_default().paths;
    if mask.is_empty() {
        return Err(rill_proto::ConvertError::missing("update_mask").into());
    }
    if let Some(bad) = mask
        .iter()
        .find(|p| !matches!(p.as_str(), "status" | "spec"))
    {
        return Err(Error::invalid(format!(
            "update_mask names {bad:?}; it may name status and spec"
        )));
    }
    let store = state.identity.store();
    let before = store
        .agent(&org, &name)
        .await?
        .ok_or_else(|| Error::NotFound(format!("agent {org}/{name}")))?;
    may_manage(state, &ctx, &before.org, &before.sponsor).await?;
    etag::check(&agent_proto(&before).etag, &req.etag, "the agent")?;
    let now = Timestamp::now();
    let mut after = match mask.iter().any(|p| p == "spec") {
        true => {
            let spec = given
                .spec
                .into_option()
                .ok_or(rill_proto::ConvertError::missing("agent.spec"))?;
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
        false => before.clone(),
    };
    if mask.iter().any(|p| p == "status") {
        after.status = match given.status.as_str() {
            "active" => AccountStatus::Active,
            "suspended" => AccountStatus::Suspended,
            other => {
                return Err(Error::invalid(format!(
                    "status is active or suspended, not {other:?}"
                )))
            }
        };
    }
    after.updated_at = now;
    store.update_agent(&before, &after, &ctx.principal).await?;
    tracing::info!(by = %ctx.principal, agent = %after.principal, status = ?after.status, "agent updated");
    Ok(pb::UpdateAgentResponse {
        agent: agent_proto(&after).into(),
    })
}

pub async fn list(
    state: &AppState,
    request: Call<pb::ListAgentsRequest>,
) -> Result<pb::ListAgentsResponse> {
    let ctx = caller(&request)?;
    let req = request.get_ref();
    let org = names::org(&req.parent)?;
    let filter = Filter::parse(
        &req.filter,
        &[("status", Kind::Str), ("sponsor", Kind::Str)],
    )?;
    let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.filter])?;
    let agents = state.identity.store().agents(&org).await?;
    let admin = require_org_admin(state, &ctx, &org).await.is_ok();
    let visible: Vec<pb::Agent> = agents
        .iter()
        .filter(|a| admin || a.sponsor == Sponsor::User(ctx.principal.name.clone()))
        .map(agent_proto)
        .filter(|a| filter.matches(&Listed(a)))
        .collect();
    let (agents, next_page_token) = page.of(visible)?;
    Ok(pb::ListAgentsResponse {
        agents,
        next_page_token,
    })
}

/// An agent as a list filter sees it.
struct Listed<'a>(&'a pb::Agent);

impl Record for Listed<'_> {
    fn field(&self, field: &str) -> FieldValue {
        match field {
            "status" => FieldValue::Str(self.0.status.clone()),
            "sponsor" => FieldValue::Str(
                self.0
                    .spec
                    .as_option()
                    .map(|s| s.sponsor.clone())
                    .unwrap_or_default(),
            ),
            _ => FieldValue::Missing,
        }
    }
}

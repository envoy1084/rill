//! `rill agent`, `rill admin trust` and `rill admin kill`.

use clap::{Args, Subcommand};
use rill_proto::rill::v1 as pb;
use serde_json::json;

use super::providers::OrgArg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Table;
use crate::Result;

/// `rill agent <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum AgentCmd {
    /// Register an agent.
    Register(Box<RegisterArgs>),
    /// The org's agents (or those you sponsor).
    List(OrgArg),
    /// Suspend an agent.
    Suspend(AgentArg),
    /// Reactivate a suspended agent.
    Activate(AgentArg),
}

/// `rill agent register`.
#[derive(Args, Debug)]
pub struct RegisterArgs {
    /// The agent's name, unique in the instance.
    pub name: String,
    /// `user:<name>` or `team:<name>`.
    #[arg(long)]
    pub sponsor: String,
    /// What it is for.
    #[arg(long, default_value = "")]
    pub purpose: String,
    /// Repositories it may use (`*` for all).
    #[arg(long, value_delimiter = ',', required = true)]
    pub repos: Vec<String>,
    /// Path prefixes it may use (default: every path).
    #[arg(long, value_delimiter = ',')]
    pub paths: Vec<String>,
    /// Models it may run as (default: any).
    #[arg(long, value_delimiter = ',')]
    pub models: Vec<String>,
    /// A workload that may act as it: `<issuer>=<subject>` (a trailing
    /// `*` in the subject matches any suffix); repeatable.
    #[arg(long = "workload", value_name = "ISSUER=SUBJECT")]
    pub workloads: Vec<String>,
    /// Change versions it may store per day.
    #[arg(long)]
    pub versions_per_day: Option<u64>,
    /// Land requests it may make per day.
    #[arg(long)]
    pub lands_per_day: Option<u64>,
    /// Requests it may make per minute, per server.
    #[arg(long)]
    pub requests_per_minute: Option<u64>,
    /// Actions it may run on remote execution per day.
    #[arg(long)]
    pub executions_per_day: Option<u64>,
    /// The organization.
    #[command(flatten)]
    pub org: OrgArg,
}

/// An agent of an org.
#[derive(Args, Debug)]
pub struct AgentArg {
    /// The agent's name.
    pub name: String,
    /// The organization.
    #[command(flatten)]
    pub org: OrgArg,
}

/// `rill admin trust <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum TrustCmd {
    /// Trust an issuer of workload JWTs.
    Add(TrustAddArgs),
    /// The org's trusted workload issuers.
    List(OrgArg),
    /// Stop trusting an issuer.
    Remove(AgentArg),
}

/// `rill admin trust add`.
#[derive(Args, Debug)]
pub struct TrustAddArgs {
    /// A name for the issuer within the org.
    pub name: String,
    /// The issuer URL (the JWTs' `iss`).
    #[arg(long)]
    pub issuer: String,
    /// Audiences the JWTs must be minted for.
    #[arg(long = "audience", required = true)]
    pub audiences: Vec<String>,
    /// A JWKS URL (default: OIDC discovery at the issuer).
    #[arg(long, conflicts_with = "jwks_file")]
    pub jwks_uri: Option<String>,
    /// A file holding the issuer's JWKS, for issuers the server cannot
    /// reach (`kubectl get --raw /openid/v1/jwks`).
    #[arg(long)]
    pub jwks_file: Option<std::path::PathBuf>,
    /// The organization.
    #[command(flatten)]
    pub org: OrgArg,
}

/// `rill admin kill`.
#[derive(Args, Debug)]
pub struct KillArgs {
    /// An agent to stop.
    #[arg(long, group = "target")]
    pub agent: Option<String>,
    /// A session (`ss_…`) to end.
    #[arg(long, group = "target")]
    pub session: Option<String>,
    /// Stop every agent of a sponsor (`user:<name>` or `team:<name>`).
    #[arg(long, group = "target")]
    pub sponsor: Option<String>,
    /// Stop every agent running as a model.
    #[arg(long, group = "target")]
    pub model: Option<String>,
    /// Why, for the record.
    #[arg(long, default_value = "")]
    pub reason: String,
    /// The organization.
    #[command(flatten)]
    pub org: OrgArg,
}

fn agent_json(a: &pb::Agent) -> serde_json::Value {
    let spec = a.spec.clone().into_option().unwrap_or_default();
    json!({
        "org": spec.org, "name": spec.name, "sponsor": spec.sponsor, "status": a.status,
        "repos": spec.repos, "paths": spec.paths, "models": spec.models,
        "workloads": spec.workloads.iter().map(|w| format!("{}={}", w.issuer, w.subject)).collect::<Vec<_>>(),
    })
}

/// Run `rill agent`.
pub async fn agent(ctx: &mut Ctx<'_>, cmd: AgentCmd) -> Result<()> {
    let client = ctx.client()?;
    let agent = match cmd {
        AgentCmd::Register(a) => {
            let workloads = a
                .workloads
                .iter()
                .map(|w| {
                    let (issuer, subject) = w.rsplit_once('=').ok_or_else(|| {
                        CliError::usage(format!("--workload {w:?}: use ISSUER=SUBJECT"))
                    })?;
                    Ok(pb::WorkloadBinding {
                        issuer: issuer.into(),
                        subject: subject.into(),
                        claims: Default::default(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            client
                .identity(&a.org.org)
                .create_agent(pb::AgentSpec {
                    org: a.org.org.clone(),
                    name: a.name,
                    sponsor: a.sponsor,
                    purpose: a.purpose,
                    repos: a.repos,
                    paths: a.paths,
                    models: a.models,
                    budgets: (pb::AgentBudgets {
                        versions_per_day: a.versions_per_day,
                        lands_per_day: a.lands_per_day,
                        requests_per_minute: a.requests_per_minute,
                        executions_per_day: a.executions_per_day,
                    })
                    .into(),
                    workloads,
                    expires_at: None,
                })
                .await?
        }
        AgentCmd::List(o) => {
            let agents = client.identity(&o.org).agents("").await?;
            return ctx.emit(
                json!(agents.iter().map(agent_json).collect::<Vec<_>>()),
                |out| {
                    let mut table = Table::new(&["NAME", "SPONSOR", "STATUS", "REPOS"]);
                    for a in &agents {
                        let spec = a.spec.clone().into_option().unwrap_or_default();
                        table.row(vec![
                            spec.name,
                            spec.sponsor,
                            a.status.clone(),
                            spec.repos.join(","),
                        ]);
                    }
                    out.table(table)
                },
            );
        }
        AgentCmd::Suspend(a) => {
            client
                .identity(&a.org.org)
                .set_agent_suspended(&a.name, true)
                .await?
        }
        AgentCmd::Activate(a) => {
            client
                .identity(&a.org.org)
                .set_agent_suspended(&a.name, false)
                .await?
        }
    };
    let value = agent_json(&agent);
    ctx.emit(value, |out| {
        let spec = agent.spec.clone().into_option().unwrap_or_default();
        out.line(format!(
            "agent {}/{} is {}",
            spec.org, spec.name, agent.status
        ))
    })
}

/// Run `rill admin trust`.
pub async fn trust(ctx: &mut Ctx<'_>, cmd: TrustCmd) -> Result<()> {
    let client = ctx.client()?;
    match cmd {
        TrustCmd::Add(a) => {
            let keys = match (a.jwks_uri, a.jwks_file) {
                (Some(uri), _) => pb::workload_issuer_spec::Keys::JwksUri(uri),
                (None, Some(path)) => {
                    pb::workload_issuer_spec::Keys::Jwks(std::fs::read_to_string(&path)?)
                }
                (None, None) => pb::workload_issuer_spec::Keys::Discovery(true),
            };
            let saved = client
                .identity(&a.org.org)
                .put_workload_issuer(pb::WorkloadIssuerSpec {
                    org: a.org.org.clone(),
                    name: a.name,
                    issuer: a.issuer,
                    keys: Some(keys),
                    audiences: a.audiences,
                    ..Default::default()
                })
                .await?;
            ctx.emit(
                json!({ "name": saved.name, "issuer": saved.issuer }),
                |out| out.line(format!("trusting {} as {}", saved.issuer, saved.name)),
            )
        }
        TrustCmd::List(o) => {
            let issuers = client.identity(&o.org).workload_issuers().await?;
            let value = json!(issuers
                .iter()
                .map(|i| json!({ "name": i.name, "issuer": i.issuer, "audiences": i.audiences }))
                .collect::<Vec<_>>());
            ctx.emit(value, |out| {
                let mut table = Table::new(&["NAME", "ISSUER", "AUDIENCES"]);
                for i in &issuers {
                    table.row(vec![
                        i.name.clone(),
                        i.issuer.clone(),
                        i.audiences.join(","),
                    ]);
                }
                out.table(table)
            })
        }
        TrustCmd::Remove(a) => {
            client
                .identity(&a.org.org)
                .delete_workload_issuer(&a.name)
                .await?;
            ctx.emit(json!({ "deleted": true }), |out| out.line("removed"))
        }
    }
}

/// Run `rill admin kill`.
pub async fn kill(ctx: &mut Ctx<'_>, a: KillArgs) -> Result<()> {
    use pb::kill_switch_request::Target;
    let target = match (a.agent, a.session, a.sponsor, a.model) {
        (Some(x), None, None, None) => Target::Agent(x),
        (None, Some(x), None, None) => Target::Session(x),
        (None, None, Some(x), None) => Target::Sponsor(x),
        (None, None, None, Some(x)) => Target::Model(x),
        _ => {
            return Err(CliError::usage(
                "name one of --agent, --session, --sponsor or --model",
            ))
        }
    };
    if !ctx.confirm("Stop these agents and end their sessions now?", true)? {
        return Err(CliError::usage("not stopped"));
    }
    let done = ctx
        .client()?
        .identity(&a.org.org)
        .kill_switch(target, &a.reason)
        .await?;
    ctx.emit(
        json!({
            "agents_suspended": done.agents_suspended,
            "sessions_revoked": done.sessions_revoked,
            "models_blocked": done.models_blocked,
        }),
        |out| {
            out.line(format!(
                "suspended {} agents, revoked {} sessions, blocked {} models",
                done.agents_suspended.len(),
                done.sessions_revoked.len(),
                done.models_blocked.len()
            ))
        },
    )
}

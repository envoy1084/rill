//! `rill intent scope | approve | revoke | explain`: the scope changes
//! under an intent may inherit its approval within, and its approvals.

use clap::Args;
use rill_mcp::json;
use rill_sdk::{FieldUpdate, IntentUpdate};
use rill_types::{IntentScope, Timestamp};

use super::intent::parse_id;
use crate::commands::review::change_or_working;
use crate::context::Ctx;
use crate::error::CliError;
use crate::Result;

/// `rill intent scope`.
#[derive(Args, Debug)]
pub struct ScopeArgs {
    /// Intent id.
    pub id: String,
    /// A glob every touched path must match (repeatable).
    #[arg(long = "path", value_name = "GLOB", required = true)]
    pub paths: Vec<String>,
    /// A glob no touched path may match (repeatable).
    #[arg(long = "forbid", value_name = "GLOB")]
    pub forbid: Vec<String>,
    /// Files a change may touch, at most.
    #[arg(long)]
    pub max_files: Option<u32>,
    /// Changes linked to the intent, at most.
    #[arg(long)]
    pub max_changes: Option<u32>,
    /// When the scope stops applying (`2026-11-15` or RFC 3339).
    #[arg(long)]
    pub expires: Option<String>,
    /// Independent agent approvals each change needs.
    #[arg(long)]
    pub agent_quorum: Option<u32>,
    /// Share of landed changes sampled for human audit, `0.05` for 5%.
    #[arg(long)]
    pub human_sample: Option<f64>,
    /// Changes linked per day, at most.
    #[arg(long)]
    pub changes_per_day: Option<u32>,
    /// Changes landed per day, at most.
    #[arg(long)]
    pub lands_per_day: Option<u32>,
}

/// `rill intent approve`.
#[derive(Args, Debug)]
pub struct ApproveArgs {
    /// Intent id.
    pub id: String,
    /// The scope revision being approved (default: the current one, as
    /// read just before approving).
    #[arg(long)]
    pub revision: Option<u32>,
}

/// `rill intent explain`.
#[derive(Args, Debug)]
pub struct ExplainArgs {
    /// Change id or version hash (default: the checkout's working change).
    pub change: Option<String>,
}

fn expires(text: &str) -> Result<Timestamp> {
    let full = if text.len() == 10 {
        format!("{text}T23:59:59Z")
    } else {
        text.to_string()
    };
    Timestamp::parse_rfc3339(&full)
        .ok_or_else(|| CliError::usage(format!("--expires {text:?} is not a date")))
}

/// Run `rill intent scope`.
pub async fn scope(ctx: &mut Ctx<'_>, args: ScopeArgs) -> Result<()> {
    let sample = args
        .human_sample
        .map(|s| {
            if (0.0..=1.0).contains(&s) {
                Ok((s * 1000.0).round() as u32)
            } else {
                Err(CliError::usage("--human-sample is a share between 0 and 1"))
            }
        })
        .transpose()?;
    let scope = IntentScope {
        paths: args.paths,
        forbid_paths: args.forbid,
        max_files_per_change: args.max_files,
        max_changes: args.max_changes,
        expires: args.expires.as_deref().map(expires).transpose()?,
        agent_quorum: args.agent_quorum,
        human_sample_permille: sample,
        changes_per_day: args.changes_per_day,
        lands_per_day: args.lands_per_day,
    };
    let update = IntentUpdate {
        scope: FieldUpdate::Set(scope),
        ..IntentUpdate::default()
    };
    let intent = ctx
        .repo()?
        .intents()
        .update(parse_id(&args.id)?, update)
        .await?;
    ctx.emit(json::intent(&intent), |out| {
        out.line(format!(
            "intent {} scope is revision {}; its owners approve it with `rill intent approve {}`",
            out.id(&intent.id.to_string()),
            intent.revision,
            intent.id
        ))
    })
}

/// Run `rill intent approve`.
pub async fn approve(ctx: &mut Ctx<'_>, args: ApproveArgs) -> Result<()> {
    let intents = ctx.repo()?.intents();
    let id = parse_id(&args.id)?;
    let revision = match args.revision {
        Some(r) => r,
        None => intents.get(id).await?.revision,
    };
    let intent = intents.approve(id, revision).await?;
    ctx.emit(json::intent(&intent), |out| {
        let owns = intent
            .approvals
            .iter()
            .rfind(|a| a.revision == revision)
            .map(|a| a.owns.join(", "))
            .unwrap_or_default();
        out.line(format!(
            "approved revision {revision} of intent {} for {}",
            out.id(&intent.id.to_string()),
            if owns.is_empty() {
                "(nothing)".into()
            } else {
                owns
            }
        ))
    })
}

/// Run `rill intent revoke`.
pub async fn revoke(ctx: &mut Ctx<'_>, id: &str) -> Result<()> {
    let intent = ctx.repo()?.intents().revoke_approval(parse_id(id)?).await?;
    ctx.emit(json::intent(&intent), |out| {
        out.line(format!(
            "withdrew your approval of intent {}",
            out.id(&intent.id.to_string())
        ))
    })
}

/// Run `rill intent explain`.
pub async fn explain(ctx: &mut Ctx<'_>, args: ExplainArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let change = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let e = repo.intents().explain_scope(change).await?;
    ctx.emit(json::intent_scope_explanation(&e), |out| {
        let Some(intent) = e.intent else {
            return out.line(out.dim("the change is linked to no intent"));
        };
        let state = match (e.approved, e.in_scope) {
            (_, true) => out.good("in scope: inherits the intent's approval"),
            (true, false) => out.warn("approved intent, but the change is outside its scope"),
            (false, _) => out.warn("the intent is not approved"),
        };
        out.line(format!("intent {}: {state}", out.id(&intent.to_string())))?;
        for r in &e.reasons {
            out.line(format!("  - {r}"))?;
        }
        if !e.missing_approvals.is_empty() {
            out.line(out.dim(&format!(
                "  waiting on owners of: {}",
                e.missing_approvals.join(", ")
            )))?;
        }
        Ok(())
    })
}

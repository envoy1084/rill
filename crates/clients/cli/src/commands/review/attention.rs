//! `rill review audits|audit|batch` and `rill intent digest`: sampled
//! audits, batch review of look-alike changes, and daily digests.

use clap::Args;
use serde_json::json;

use super::super::intent::parse_id;
use super::super::parse::seq_arg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Table;
use crate::Result;

/// `rill review audit`.
#[derive(Args, Debug)]
pub struct AuditArgs {
    /// The landing: `trunk@N` or `N`.
    pub landing: String,
    /// It holds up.
    #[arg(long, conflicts_with = "fail")]
    pub pass: bool,
    /// It should not have landed.
    #[arg(long)]
    pub fail: bool,
    /// With `--fail`, also revert it.
    #[arg(long, requires = "fail")]
    pub revert: bool,
    /// What you found.
    #[arg(short, long, default_value = "")]
    pub message: String,
}

/// `rill review batch`.
#[derive(Args, Debug)]
pub struct BatchArgs {
    /// The intent whose changes to group.
    #[arg(long)]
    pub intent: String,
    /// Vote on every change of this group (its signature).
    #[arg(long)]
    pub approve: Option<String>,
    /// The vote (default +2 on `Review`).
    #[arg(long, default_value_t = 2, allow_hyphen_values = true)]
    pub value: i8,
}

/// `rill intent digest`.
#[derive(Args, Debug)]
pub struct DigestArgs {
    /// Intent id.
    pub id: String,
    /// `YYYY-MM-DD`, UTC (default today).
    #[arg(long, default_value = "")]
    pub day: String,
}

fn tail(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// List audits waiting for a human.
pub async fn audits(ctx: &mut Ctx<'_>) -> Result<()> {
    let audits = ctx.repo()?.attention().audits().await?;
    let value = json!(audits
        .iter()
        .map(|a| json!({ "landing": tail(&a.landing), "change": tail(&a.change), "risk": a.risk, "assignees": a.assignees }))
        .collect::<Vec<_>>());
    ctx.emit(value, |out| {
        if audits.is_empty() {
            return out.line(out.dim("no audits waiting"));
        }
        let mut table = Table::new(&["LANDING", "CHANGE", "RISK", "ASSIGNED TO"]);
        for a in &audits {
            table.row(vec![
                format!("trunk@{}", tail(&a.landing)),
                tail(&a.change).to_string(),
                a.risk.clone(),
                a.assignees.join(", "),
            ]);
        }
        out.table(table)
    })
}

/// Pass or fail an audit.
pub async fn audit(ctx: &mut Ctx<'_>, a: AuditArgs) -> Result<()> {
    if a.pass == a.fail {
        return Err(CliError::usage("say --pass or --fail"));
    }
    let seq = seq_arg(&a.landing)?;
    let done = ctx
        .repo()?
        .attention()
        .record_audit(seq, a.pass, &a.message, a.revert)
        .await?;
    ctx.emit(json!({ "landing": seq.0, "status": done.status }), |out| {
        out.line(format!("trunk@{}: audit {}", seq.0, done.status))
    })
}

/// Group an intent's changes by edit, or vote on a group.
pub async fn batch(ctx: &mut Ctx<'_>, a: BatchArgs) -> Result<()> {
    let intent = parse_id(&a.intent)?;
    let attention = ctx.repo()?.attention();
    if let Some(signature) = a.approve {
        let results = attention
            .batch_vote(intent, &signature, "Review", a.value, "batch review")
            .await?;
        let value = json!(results
            .iter()
            .map(|r| json!({ "change": tail(&r.change), "voted": r.voted, "error": r.error }))
            .collect::<Vec<_>>());
        return ctx.emit(value, |out| {
            for r in &results {
                let state = if r.voted {
                    out.good("voted")
                } else {
                    out.warn(&r.error)
                };
                out.line(format!("  {} {state}", tail(&r.change)))?;
            }
            Ok(())
        });
    }
    let groups = attention.similar(intent).await?;
    let value = json!(groups
        .iter()
        .map(|g| json!({ "signature": g.signature, "changes": g.changes.iter().map(|c| tail(c)).collect::<Vec<_>>() }))
        .collect::<Vec<_>>());
    ctx.emit(value, |out| {
        if groups.is_empty() {
            return out.line(out.dim("no changes of this intent are in review"));
        }
        let mut table = Table::new(&["SIGNATURE", "CHANGES", "FIRST"]);
        for g in &groups {
            table.row(vec![
                out.bold(&g.signature),
                g.changes.len().to_string(),
                g.changes.first().map(|c| tail(c).to_string()).unwrap_or_default(),
            ]);
        }
        out.table(table)?;
        out.line(out.dim(
            "review one change of a group, then `rill review batch --intent <id> --approve <signature>`",
        ))
    })
}

/// An intent's day.
pub async fn digest(ctx: &mut Ctx<'_>, a: DigestArgs) -> Result<()> {
    let id = parse_id(&a.id)?;
    let d = ctx.repo()?.attention().digest(id, &a.day).await?;
    ctx.emit(
        json!({ "day": d.day, "landed": d.landed, "reverted": d.reverted, "flagged": d.flagged, "sampled": d.sampled, "open": d.open }),
        |out| {
            out.line(format!(
                "{}: {} landed, {} reverted, {} flagged by reviewers, {} sampled for audit; {} still in review",
                d.day, d.landed, d.reverted, d.flagged, d.sampled, d.open
            ))
        },
    )
}

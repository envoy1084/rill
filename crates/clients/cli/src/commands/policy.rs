//! `rill policy` (the rules in force, simulating a candidate, org
//! guardrails) and `rill why` (the decision behind a landing).

use std::path::PathBuf;

use clap::{Args, Subcommand};
use serde_json::json;

use super::identity::providers::OrgArg;
use super::parse::seq_arg;
use crate::context::Ctx;
use crate::output::Table;
use crate::Result;

/// `rill policy <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum PolicyCmd {
    /// The submit rules in force and where they come from.
    Show,
    /// Replay landed changes under a candidate policy.
    Simulate(SimulateArgs),
    /// An org's guardrails: forbid rules above every repository's policy.
    #[command(subcommand)]
    Guardrails(GuardrailsCmd),
}

/// `rill policy simulate`.
#[derive(Args, Debug)]
pub struct SimulateArgs {
    /// The candidate Cedar file.
    #[arg(long)]
    pub policy: PathBuf,
    /// First landing to replay (`trunk@N` or `N`; default the first).
    #[arg(long)]
    pub since: Option<String>,
    /// Last landing to replay (default the newest).
    #[arg(long)]
    pub until: Option<String>,
}

/// `rill policy guardrails <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum GuardrailsCmd {
    /// Print the org's guardrails.
    Get(OrgArg),
    /// Replace them from a file (an empty file removes them).
    Set(GuardrailsSetArgs),
}

/// `rill policy guardrails set`.
#[derive(Args, Debug)]
pub struct GuardrailsSetArgs {
    /// Cedar `forbid` rules, each `@id("org_…")`.
    pub file: PathBuf,
    /// The organization.
    #[command(flatten)]
    pub org: OrgArg,
}

/// `rill why`.
#[derive(Args, Debug)]
pub struct WhyArgs {
    /// The landing: `trunk@N` or `N`.
    pub landing: String,
    /// Also print the facts the decision saw.
    #[arg(long)]
    pub facts: bool,
}

/// Run `rill policy`.
pub async fn run(ctx: &mut Ctx<'_>, cmd: PolicyCmd) -> Result<()> {
    let admin = ctx.client()?.admin();
    match cmd {
        PolicyCmd::Show => {
            let repo = ctx.repo_name()?;
            let p = admin.effective_policy(&repo).await?;
            ctx.emit(
                json!({ "source": p.source, "digest": p.digest, "text": p.text }),
                |out| {
                    out.line(format!("{} {}", out.bold("source:"), p.source))?;
                    out.line(format!("{} {}", out.bold("digest:"), p.digest))?;
                    out.line(&p.text)
                },
            )
        }
        PolicyCmd::Simulate(a) => {
            let repo = ctx.repo_name()?;
            let text = std::fs::read_to_string(&a.policy)?;
            let bound = |s: Option<String>| -> Result<u64> {
                Ok(match s {
                    Some(s) => seq_arg(&s)?.0,
                    None => 0,
                })
            };
            let r = admin
                .simulate_policy(&repo, &text, bound(a.since)?, bound(a.until)?)
                .await?;
            let blocked: Vec<_> = r
                .blocked
                .iter()
                .map(|b| json!({ "landing": b.landing, "change": b.change, "reasons": b.reasons }))
                .collect();
            ctx.emit(
                json!({
                    "replayed": r.replayed, "unrecorded": r.unrecorded, "allowed": r.allowed,
                    "blocked": blocked,
                    "agent_only_before": r.agent_only_before, "agent_only_after": r.agent_only_after,
                }),
                |out| {
                    out.line(format!(
                        "replayed {} landings ({} without a recorded decision)",
                        r.replayed, r.unrecorded
                    ))?;
                    out.line(format!(
                        "still allowed: {}   newly blocked: {}",
                        r.allowed,
                        r.blocked.len()
                    ))?;
                    out.line(format!(
                        "landable on agent review alone: {} before, {} after",
                        r.agent_only_before, r.agent_only_after
                    ))?;
                    if r.blocked.is_empty() {
                        return Ok(());
                    }
                    let mut table = Table::new(&["LANDING", "CHANGE", "WHY"]);
                    for b in &r.blocked {
                        let landing = b.landing.rsplit('/').next().unwrap_or_default();
                        let change = b.change.rsplit('/').next().unwrap_or_default();
                        table.row(vec![
                            format!("trunk@{landing}"),
                            change.to_string(),
                            b.reasons.join("; "),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        PolicyCmd::Guardrails(GuardrailsCmd::Get(o)) => {
            let text = admin.guardrails(&o.org).await?;
            ctx.emit(json!({ "org": o.org, "text": text }), |out| {
                if text.is_empty() {
                    out.line(out.dim("no guardrails"))
                } else {
                    out.line(&text)
                }
            })
        }
        PolicyCmd::Guardrails(GuardrailsCmd::Set(a)) => {
            let text = std::fs::read_to_string(&a.file)?;
            let saved = admin.set_guardrails(&a.org.org, &text).await?;
            ctx.emit(json!({ "org": a.org.org, "text": saved }), |out| {
                out.line(format!("guardrails of {} updated", a.org.org))
            })
        }
    }
}

/// Run `rill why`.
pub async fn why(ctx: &mut Ctx<'_>, args: WhyArgs) -> Result<()> {
    let repo = ctx.repo_name()?;
    let seq = seq_arg(&args.landing)?;
    let w = ctx.client()?.admin().explain_landing(&repo, seq).await?;
    let facts: serde_json::Value = serde_json::from_str(&w.facts_json).unwrap_or_default();
    ctx.emit(
        json!({
            "landing": seq.0, "change": w.change, "allowed": w.allowed, "policy": w.policy,
            "source": w.source, "matched_policies": w.matched_policies, "reasons": w.reasons,
            "facts": facts,
        }),
        |out| {
            let change = w.change.rsplit('/').next().unwrap_or_default();
            out.line(format!(
                "trunk@{} ({change}) landed under {} [{}]",
                seq.0,
                w.source,
                &w.policy[..w.policy.len().min(12)]
            ))?;
            for r in &w.reasons {
                out.line(format!("  {r}"))?;
            }
            if args.facts {
                out.line(&w.facts_json)?;
            }
            Ok(())
        },
    )
}

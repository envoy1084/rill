//! `rill owners`: who owns a path (trunk's `OWNERS.toml` files), and where
//! a change stands with each owner set it touches.

use clap::{Args, Subcommand};
use rill_mcp::json;
use rill_types::RepoPathBuf;

use crate::commands::review::change_or_working;
use crate::context::Ctx;
use crate::output::Table;
use crate::Result;

/// `rill owners <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum OwnersCmd {
    /// The owner sets covering a path on trunk.
    Show(ShowArgs),
    /// Each owner set a change touches: approvals, required, escalation.
    Explain(ExplainArgs),
}

/// `rill owners show`.
#[derive(Args, Debug)]
pub struct ShowArgs {
    /// A file path.
    pub path: String,
}

/// `rill owners explain`.
#[derive(Args, Debug)]
pub struct ExplainArgs {
    /// Change id or version hash (default: the checkout's working change).
    pub change: Option<String>,
}

/// Run an owners subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: OwnersCmd) -> Result<()> {
    match cmd {
        OwnersCmd::Show(args) => show(ctx, args).await,
        OwnersCmd::Explain(args) => explain(ctx, args).await,
    }
}

async fn show(ctx: &mut Ctx<'_>, args: ShowArgs) -> Result<()> {
    let path = RepoPathBuf::new(&args.path)?;
    let sets = ctx.repo()?.reviews().owners_of(&path).await?;
    ctx.emit(
        serde_json::json!({ "path": path.as_str(), "sets": sets.iter().map(json::owner_set).collect::<Vec<_>>() }),
        |out| {
            if sets.is_empty() {
                return out.line(out.dim(&format!(
                    "no OWNERS.toml covers {path}; owners come from the `owner` relation"
                )));
            }
            let mut table = Table::new(&["SET", "OWNERS", "REQUIRED", "SLA", "ESCALATES TO"]);
            for s in &sets {
                table.row(vec![
                    out.bold(&s.id),
                    s.owners.join(", "),
                    s.required.to_string(),
                    s.sla.map(|d| format!("{}m", d.as_secs() / 60)).unwrap_or_default(),
                    s.escalate_to.clone().unwrap_or_default(),
                ]);
            }
            out.table(table)
        },
    )
}

async fn explain(ctx: &mut Ctx<'_>, args: ExplainArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let ownership = repo.reviews().ownership(id).await?;
    ctx.emit(json::ownership(&ownership), |out| {
        if ownership.sets.is_empty() {
            return out.line(out.dim("the change touches no OWNERS.toml owner set"));
        }
        let mut table = Table::new(&["SET", "OWNERS", "APPROVALS", "STATE", "ESCALATED"]);
        for s in &ownership.sets {
            let state = if s.covered {
                out.good("covered")
            } else {
                out.warn("waiting")
            };
            table.row(vec![
                out.bold(&s.set.id),
                s.set.owners.join(", "),
                format!("{}/{}", s.approvals, s.set.required),
                state,
                s.escalated_to
                    .as_ref()
                    .map(|to| format!("{to} (×{})", s.escalations))
                    .unwrap_or_default(),
            ]);
        }
        out.table(table)
    })
}

//! `rill review interdiff|apply|reopen`.

use clap::Args;
use serde_json::json;

use super::args::ChangeArg;
use super::working_change::change_or_working;
use crate::commands::parse::change_ref;
use crate::context::Ctx;
use crate::Result;

/// `rill review interdiff`.
#[derive(Args, Debug)]
pub struct InterdiffArgs {
    /// Change id or version hash (default: the checkout's working change).
    pub change: Option<String>,
    /// The older version (default: the one before `--to`).
    #[arg(long)]
    pub from: Option<String>,
    /// The newer version (default: the current one).
    #[arg(long)]
    pub to: Option<String>,
}

/// `rill review apply`.
#[derive(Args, Debug)]
pub struct ApplyArgs {
    /// The thread whose suggestion to apply.
    pub thread: String,
    /// Change id or version hash (default: the checkout's working change).
    #[arg(long)]
    pub change: Option<String>,
}

/// Show what changed between two versions.
pub async fn interdiff(ctx: &mut Ctx<'_>, a: InterdiffArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, a.change.as_deref(), &repo).await?;
    let hash = |s: Option<String>| -> Result<Option<rill_types::ChangeVersionHash>> {
        s.map(|s| s.parse().map_err(Into::into)).transpose()
    };
    let d = repo
        .reviews()
        .interdiff(id, hash(a.from)?, hash(a.to)?)
        .await?;
    let value = json!({
        "from": d.from, "to": d.to, "rebased": d.rebased,
        "files": d.files.iter().map(|f| json!({ "path": f.path, "added": f.added, "deleted": f.deleted, "unified": f.unified })).collect::<Vec<_>>(),
    });
    ctx.emit(value, |out| {
        if d.files.is_empty() {
            out.line(out.dim("the author changed nothing between these versions"))?;
        }
        for f in &d.files {
            out.raw(f.unified.as_bytes())?;
        }
        if !d.rebased.is_empty() {
            out.line(out.dim(&format!(
                "unchanged apart from the rebase: {}",
                d.rebased.join(", ")
            )))?;
        }
        Ok(())
    })
}

/// Apply a suggestion.
pub async fn apply(ctx: &mut Ctx<'_>, a: ApplyArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, a.change.as_deref(), &repo).await?;
    let version = repo.reviews().apply_suggestion(id, &a.thread).await?;
    ctx.emit(json!({ "version": version.to_hex() }), |out| {
        out.line(format!(
            "applied the suggestion as version {}",
            out.id(&version.to_hex()[..12])
        ))
    })
}

/// Reopen an abandoned change.
pub async fn reopen(ctx: &mut Ctx<'_>, a: ChangeArg) -> Result<()> {
    let repo = ctx.repo()?;
    let id = match a.change.as_deref() {
        Some(c) => change_ref(c)?.id(&repo).await?,
        None => change_or_working(ctx, None, &repo).await?,
    };
    let state = repo.reviews().reopen(id).await?;
    ctx.emit(
        json!({ "change": id.to_string(), "status": rill_mcp::json::enum_name(&state.status) }),
        |out| out.line(format!("reopened {id}")),
    )
}

//! `rill watch`: stream trunk landings, ref moves or new versions as
//! lines (one JSON object per line with `--json`).

use clap::{Args, Subcommand};
use futures::StreamExt;
use rill_mcp::json;
use serde_json::Value;

use crate::commands::seq_arg;
use crate::context::Ctx;
use crate::output::{short, when};
use crate::Result;

/// `rill watch <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum WatchCmd {
    /// Landings on trunk.
    Trunk(TrunkArgs),
    /// Ref creations, moves and deletions.
    Refs(RefsArgs),
    /// New change versions as they are created.
    Log(TrunkArgs),
}

/// `rill watch trunk` / `rill watch log`.
#[derive(Args, Debug)]
pub struct TrunkArgs {
    /// Start after this sequence (default: now).
    #[arg(long)]
    pub after: Option<String>,
}

/// `rill watch refs`.
#[derive(Args, Debug)]
pub struct RefsArgs {
    /// Only refs starting with this prefix.
    pub prefix: Option<String>,
}

/// Run a watch subcommand. Streams until the server closes the stream or
/// the process is interrupted.
pub async fn run(ctx: &mut Ctx<'_>, cmd: WatchCmd) -> Result<()> {
    let repo = ctx.repo()?;
    match cmd {
        WatchCmd::Trunk(args) => {
            let after = args.after.as_deref().map(seq_arg).transpose()?;
            let mut stream = repo.watch_trunk(after).await?;
            while let Some(event) = stream.next().await {
                let e = event?;
                emit(ctx, json::land_event(&e), || {
                    format!(
                        "trunk@{} {} {} by {} at {}",
                        e.seq.get(),
                        short(&e.hash),
                        e.change_id,
                        e.landed_by,
                        when(&e.at)
                    )
                })?;
            }
        }
        WatchCmd::Refs(args) => {
            let mut stream = repo.watch_refs(args.prefix.as_deref()).await?;
            while let Some(event) = stream.next().await {
                let e = event?;
                emit(ctx, json::ref_event(&e), || {
                    format!(
                        "{} {} -> {} at {}",
                        e.name,
                        e.old.map(|h| short(&h)).unwrap_or_else(|| "(new)".into()),
                        e.new
                            .map(|h| short(&h))
                            .unwrap_or_else(|| "(deleted)".into()),
                        when(&e.at)
                    )
                })?;
            }
        }
        WatchCmd::Log(args) => {
            let after = args.after.as_deref().map(seq_arg).transpose()?;
            let mut stream = repo.watch_log(after).await?;
            while let Some(event) = stream.next().await {
                let e = event?;
                emit(ctx, json::change_info(&e), || {
                    format!("{} {} {}", short(&e.hash), e.author, e.summary())
                })?;
            }
        }
    }
    Ok(())
}

fn emit(ctx: &mut Ctx<'_>, value: Value, human: impl FnOnce() -> String) -> Result<()> {
    if ctx.json() {
        ctx.emit_json_line(&value)
    } else {
        let line = human();
        ctx.out().line(line)?;
        ctx.out().flush()?;
        Ok(())
    }
}

//! `rill release`: cut release refs and list them. A release moves only
//! through its lander: land on it with `rill land --onto release/<name>`.

use clap::{Args, Subcommand};
use rill_mcp::json;
use rill_types::TrunkSeq;
use serde_json::json as j;

use crate::context::Ctx;
use crate::output::{short, Table};
use crate::Result;

/// `rill release <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ReleaseCmd {
    /// Cut `release/<name>` at a trunk position (repository owners only).
    Cut(CutArgs),
    /// List releases and their heads.
    List,
}

/// `rill release cut`.
#[derive(Args, Debug)]
pub struct CutArgs {
    /// Release name: the ref is `release/<name>`.
    pub name: String,
    /// Trunk position to cut at (default: the head).
    #[arg(long, value_name = "SEQ")]
    pub at: Option<String>,
}

/// Run a release subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: ReleaseCmd) -> Result<()> {
    match cmd {
        ReleaseCmd::Cut(args) => cut(ctx, args).await,
        ReleaseCmd::List => list(ctx).await,
    }
}

async fn cut(ctx: &mut Ctx<'_>, args: CutArgs) -> Result<()> {
    let at = args
        .at
        .as_deref()
        .map(|s| s.parse::<TrunkSeq>())
        .transpose()
        .map_err(|e| crate::error::CliError::usage(format!("--at: {e}")))?;
    let (name, hash, seq) = ctx.repo()?.cut_release(&args.name, at).await?;
    ctx.emit(
        j!({ "ref": name.as_str(), "target": hash.to_hex(), "trunk_seq": seq.get() }),
        |out| {
            out.line(format!(
                "cut {} at trunk@{} ({}); land on it with `rill land --onto {}`",
                out.bold(name.as_str()),
                seq.get(),
                short(&hash),
                name
            ))
        },
    )
}

async fn list(ctx: &mut Ctx<'_>) -> Result<()> {
    let refs = ctx.repo()?.list_refs(Some("release/")).await?;
    ctx.emit(
        j!({ "releases": refs.iter().map(json::ref_info).collect::<Vec<_>>() }),
        |out| {
            if refs.is_empty() {
                return out.line(out.dim("no releases"));
            }
            let mut table = Table::new(&["RELEASE", "HEAD"]);
            for r in &refs {
                table.row(vec![out.bold(r.name.as_str()), out.id(&short(&r.target))]);
            }
            out.table(table)
        },
    )
}

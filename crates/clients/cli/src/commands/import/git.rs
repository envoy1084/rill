//! `rill import git`: one-way import of a git repository into a running
//! server through its API. Imports straight into a storage directory are
//! `rill-server import git`.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use rill_git_import::{GixSource, ImportOptions, Importer};
use serde_json::json as j;

use super::remote::RemoteTarget;
use crate::context::Ctx;
use crate::Result;

/// `rill import <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ImportCmd {
    /// Import a git repository's history into a server.
    Git(GitArgs),
}

/// `rill import git`.
#[derive(Args, Debug)]
pub struct GitArgs {
    /// Path to the git repository (worktree or bare).
    pub path: PathBuf,
    /// Rill repository to import into (created if missing).
    #[arg(long)]
    pub repo: String,
    /// Git ref to import from.
    #[arg(long, default_value = "HEAD")]
    pub r#ref: String,
    /// Where an import through a server keeps its sha map (default: inside
    /// the git directory).
    #[arg(long)]
    pub map_file: Option<PathBuf>,
    /// Only convert commits; do not land the mainline on trunk.
    #[arg(long)]
    pub no_land: bool,
}

/// Run an import subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: ImportCmd) -> Result<()> {
    match cmd {
        ImportCmd::Git(args) => git(ctx, args).await,
    }
}

async fn git(ctx: &mut Ctx<'_>, args: GitArgs) -> Result<()> {
    let options = ImportOptions {
        land_first_parent_history: !args.no_land,
        ..ImportOptions::default()
    };
    let source = GixSource::open(&args.path)?;
    let client = ctx.client()?;
    if client.repo(&args.repo).info().await.is_err() {
        client.create_repo(&args.repo, None).await?;
    }
    let map_file = args.map_file.clone().unwrap_or_else(|| {
        let git_dir = args.path.join(".git");
        let dir = if git_dir.is_dir() {
            git_dir
        } else {
            args.path.clone()
        };
        dir.join(format!("rill-import-{}.json", args.repo))
    });
    let target = RemoteTarget::new(client, args.repo.clone(), map_file);
    let mut importer = Importer::open(target).await?;
    let options = ImportOptions {
        save_every: 1,
        ..options
    };
    let repo_name = args.repo.clone();
    let report = importer.import(source, &args.r#ref, options).await?;
    ctx.emit(
        j!({
            "repo": repo_name,
            "imported": report.imported,
            "skipped": report.skipped,
            "landed": report.landed,
            "head": report.head.map(|h| h.to_hex()),
            "trunk_head": report.trunk_head.map(|h| h.to_hex()),
            "seq": report.seq.map(|s| s.get()),
            "warnings": report.warnings,
        }),
        |out| {
            out.line(format!(
                "imported {} commit(s) into {} ({} already present, {} landed)",
                report.imported,
                out.bold(&repo_name),
                report.skipped,
                report.landed
            ))?;
            if let Some(seq) = report.seq {
                out.line(format!("trunk is now trunk@{}", seq.get()))?;
            }
            for w in &report.warnings {
                out.line(out.warn(&format!("warning: {w}")))?;
            }
            Ok(())
        },
    )
}

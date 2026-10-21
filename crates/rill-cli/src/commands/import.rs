//! `rill import git`: one-way import of a git repository.
//!
//! Import runs directly against a storage directory (the same layout
//! `rill api --storage local:<dir>` uses), not through the API: it
//! writes thousands of objects and needs the importer's sha map, neither of
//! which the client API exposes. Run it while the server is stopped, or
//! against a directory the server will be started on afterwards. Passing
//! `--endpoint` is refused so nobody expects a remote import.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use rill_git_import::{GixSource, ImportOptions, Importer};
use rill_store::LocalStores;
use serde_json::json as j;

use crate::context::Ctx;
use crate::error::CliError;
use crate::Result;

/// `rill import <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ImportCmd {
    /// Import a git repository's history into local storage.
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
    /// Storage directory (the server's `local:<dir>`).
    #[arg(long, env = "RILL_STORAGE_DIR")]
    pub storage_dir: PathBuf,
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
    if ctx.opts().endpoint.is_some() {
        return Err(CliError::usage(
            "import runs directly against a storage directory, not a server: drop --endpoint and pass --storage-dir",
        ));
    }
    let stores = LocalStores::open(&args.storage_dir).await?;
    stores.check_format().await?;
    let config = match stores.get_repo_by_name(&args.repo).await? {
        Some(c) => c,
        None => stores.create_repo(&args.repo, "").await?,
    };
    let source = GixSource::open(&args.path)?;
    let mut importer = Importer::open(stores.repo(config.id)).await?;
    let options = ImportOptions {
        land_first_parent_history: !args.no_land,
        ..ImportOptions::default()
    };
    let report = importer.import(source, &args.r#ref, options).await?;
    ctx.emit(
        j!({
            "repo": config.name,
            "repo_id": config.id.to_string(),
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
                out.bold(&config.name),
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

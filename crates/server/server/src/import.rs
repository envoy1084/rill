//! `rill-server import git`: a one-way import of a git repository straight
//! into a storage directory (the layout `run --storage local:<dir>` uses)
//! while no server has it open. Imports into a running server go through
//! its API with `rill import git`.

use std::io::Write;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use rill_git_import::{GixSource, ImportOptions, Importer};
use rill_store::LocalStores;

use crate::error::Result;

/// `rill-server import <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ImportCmd {
    /// Import a git repository's history into a storage directory.
    Git(GitArgs),
}

/// `rill-server import git`.
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
    /// The storage directory (the server's `local:<dir>`).
    #[arg(long, env = "RILL_STORAGE_DIR")]
    pub storage_dir: PathBuf,
    /// Only convert commits; do not land the mainline on trunk.
    #[arg(long)]
    pub no_land: bool,
}

/// Run an import subcommand, reporting what it did.
pub async fn run(cmd: ImportCmd, out: &mut dyn Write) -> Result<()> {
    let ImportCmd::Git(args) = cmd;
    let options = ImportOptions {
        land_first_parent_history: !args.no_land,
        ..ImportOptions::default()
    };
    let source = GixSource::open(&args.path)?;
    let stores = LocalStores::open(&args.storage_dir).await?;
    stores.check_format().await?;
    let config = match stores.get_repo_by_name(&args.repo).await? {
        Some(c) => c,
        None => stores.create_repo(&args.repo, "").await?,
    };
    let mut importer = Importer::open(stores.repo(config.id)).await?;
    let report = importer.import(source, &args.r#ref, options).await?;
    writeln!(
        out,
        "imported {} commit(s) into {} ({} already present, {} landed)",
        report.imported, config.name, report.skipped, report.landed
    )?;
    if let Some(seq) = report.seq {
        writeln!(out, "trunk is now trunk@{}", seq.get())?;
    }
    for w in &report.warnings {
        writeln!(out, "warning: {w}")?;
    }
    Ok(())
}

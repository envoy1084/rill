//! `rill checkout`: ask the daemon to check out a revision into a
//! directory, with a server workspace it keeps in sync.

use std::path::PathBuf;

use clap::Args;
use rill_daemon::proto as dpb;
use serde_json::json as j;

use crate::context::Ctx;
use crate::Result;

/// `rill checkout`.
#[derive(Args, Debug)]
pub struct CheckoutArgs {
    /// Repository name or id.
    pub repo: String,
    /// Directory to create (must be empty or absent).
    pub dir: PathBuf,
    /// Revision to check out: trunk, trunk@N, a version hash or a ref.
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
    /// Only check out these directory prefixes (repeatable).
    #[arg(long, value_name = "PREFIX")]
    pub sparse: Vec<String>,
    /// Do not register a server-side workspace.
    #[arg(long)]
    pub no_workspace: bool,
    /// Present files lazily through the virtual filesystem (Linux).
    #[arg(long, conflicts_with = "snapshot")]
    pub lazy: bool,
    /// A read-only view of the revision for CI: writes stay in a scratch
    /// overlay that is never synced (Linux; no workspace).
    #[arg(long)]
    pub snapshot: bool,
}

/// Run `rill checkout`.
pub async fn run(ctx: &mut Ctx<'_>, args: CheckoutArgs) -> Result<()> {
    let dir = if args.dir.is_absolute() {
        args.dir.clone()
    } else {
        ctx.cwd().join(&args.dir)
    };
    let endpoint = ctx.client_config()?.endpoint;
    let token = ctx.daemon_token();
    let mode = match (args.lazy, args.snapshot) {
        (_, true) => dpb::MountMode::Snapshot,
        (true, false) => dpb::MountMode::Lazy,
        (false, false) => dpb::MountMode::Eager,
    };
    let created = ctx
        .daemon()
        .await?
        .create_mount(dpb::CreateMountRequest {
            path: dir.display().to_string(),
            endpoint,
            token,
            repo: args.repo.clone(),
            revision: args.at.clone().unwrap_or_default(),
            sparse: args.sparse.clone(),
            mode: mode.into(),
            no_workspace: args.no_workspace,
        })
        .await?;
    let mount = created.mount.into_option().unwrap_or_default();
    let files = created.files;
    ctx.emit(
        j!({
            "dir": mount.path,
            "repo": args.repo,
            "mount": mount.name,
            "base": mount.base,
            "base_version": (!mount.base_version.is_empty()).then_some(&mount.base_version),
            "files": files,
            "workspace": (!mount.workspace.is_empty()).then_some(&mount.workspace),
        }),
        |out| {
            let version = if mount.base_version.is_empty() {
                String::new()
            } else {
                format!(" ({})", &mount.base_version[..12])
            };
            out.line(format!(
                "checked out {} at {}{version} into {} ({files} files)",
                out.bold(&args.repo),
                mount.base,
                mount.path
            ))?;
            if !mount.workspace.is_empty() {
                out.line(format!("workspace {}", out.id(&mount.workspace)))?;
            }
            Ok(())
        },
    )
}

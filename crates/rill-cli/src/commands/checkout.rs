//! `rill checkout`: materialize a root into a directory and register a
//! server-side workspace for it.

use std::path::PathBuf;

use clap::Args;
use rill_mcp::json;
use rill_sdk::{Checkout, Root};
use rill_types::{Encodable, RepoPathBuf, SparseProfile, Tree};
use serde_json::json as j;

use crate::commands::root_arg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::Result;

/// `rill checkout`.
#[derive(Args, Debug)]
pub struct CheckoutArgs {
    /// Repository name or id.
    pub repo: String,
    /// Directory to create (must be empty or absent).
    pub dir: PathBuf,
    /// Root to materialize: trunk, trunk@N, a version hash or a ref.
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
    /// Only materialize these directory prefixes (repeatable).
    #[arg(long, value_name = "PREFIX")]
    pub sparse: Vec<String>,
    /// Do not register a server-side workspace.
    #[arg(long)]
    pub no_workspace: bool,
}

/// Run `rill checkout`.
pub async fn run(ctx: &mut Ctx<'_>, args: CheckoutArgs) -> Result<()> {
    let root = root_arg(args.at.as_deref())?;
    let sparse = SparseProfile {
        include: args
            .sparse
            .iter()
            .map(RepoPathBuf::new)
            .collect::<std::result::Result<_, _>>()?,
        exclude: Vec::new(),
    };
    let dir = if args.dir.is_absolute() {
        args.dir.clone()
    } else {
        ctx.cwd().join(&args.dir)
    };
    let repo = ctx.client()?.repo(args.repo.clone());
    // Fail early with a clear message when the repository does not exist,
    // rather than after creating the directory.
    repo.info().await?;

    // An empty repository has no trunk ref to materialize. Its checkout is
    // based on the empty tree, which the server stores when a workspace is
    // created without a base, so the workspace is registered first.
    let empty = args.at.is_none() && trunk_is_empty(&repo).await?;
    let root = if empty {
        if args.no_workspace {
            return Err(CliError::usage(
                "the repository is empty: checking it out needs a workspace (drop --no-workspace)",
            ));
        }
        Root::Tree(Tree::empty().hash())
    } else {
        root
    };
    let mut workspace = None;
    if !args.no_workspace {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "checkout".to_string());
        let base = if empty { None } else { Some(root.clone()) };
        let created = repo
            .workspaces()
            .create(&name, base, sparse.clone())
            .await?;
        workspace = Some(created.workspace);
    }
    let mut checkout = Checkout::create(&dir, repo.clone(), root.clone(), sparse).await?;
    if let Some(ws) = &workspace {
        checkout.set_workspace(Some(ws.id)).await?;
    }
    let files = checkout.manifest().len();
    let base_version = checkout.meta().base_version;
    ctx.emit(
        j!({
            "dir": dir.display().to_string(),
            "repo": args.repo,
            "base": root.to_string(),
            "base_version": base_version.map(|h| h.to_hex()),
            "files": files,
            "workspace": workspace.as_ref().map(json::workspace),
        }),
        |out| {
            out.line(format!(
                "checked out {} at {} into {} ({files} files)",
                out.bold(&args.repo),
                describe_root(&root, base_version),
                dir.display()
            ))?;
            if let Some(ws) = &workspace {
                out.line(format!(
                    "workspace {} ({})",
                    ws.name,
                    out.id(&ws.id.to_string())
                ))?;
            }
            Ok(())
        },
    )
}

fn describe_root(root: &Root, version: Option<rill_types::ChangeVersionHash>) -> String {
    match version {
        Some(v) => format!("{root} ({})", crate::output::short(&v)),
        None => root.to_string(),
    }
}

/// True when trunk has no head. The server reports this either as an
/// empty answer or as `NotFound`, depending on version.
async fn trunk_is_empty(repo: &rill_sdk::RepoClient) -> Result<bool> {
    match repo.trunk_head().await {
        Ok(head) => Ok(head.is_none()),
        Err(e) if e.is_not_found() => Ok(true),
        Err(e) => Err(e.into()),
    }
}

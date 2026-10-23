//! `rill squash` and `rill split`: reshaping the working change, on the
//! server, with the checkout's files left exactly as they are.

use clap::Args;
use rill_daemon::proto as dpb;
use rill_mcp::json;
use rill_sdk::{ApplyOptions, Edit, EditBase, Root, VersionResult};
use rill_types::{ChangeVersion, ChangeVersionHash, FileChange, RepoPathBuf, Tree};
use serde_json::json as j;

use super::render::print_version;
use super::snapshot::commit;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Role;
use crate::Result;

/// `rill squash`.
#[derive(Args, Debug)]
pub struct SquashArgs {
    /// Message of the combined change (default: keep the parent's).
    #[arg(short, long)]
    pub message: Option<String>,
}

/// `rill split`.
#[derive(Args, Debug)]
pub struct SplitArgs {
    /// Paths or directory prefixes that move into a new change below.
    #[arg(required = true)]
    pub paths: Vec<String>,
    /// Message of the new change (default: "Split from" the working
    /// change's summary).
    #[arg(short, long)]
    pub message: Option<String>,
}

/// The edits that make `cv`'s file changes, for paths `keep` accepts.
fn edits_of(cv: &ChangeVersion, keep: impl Fn(&RepoPathBuf) -> bool) -> Result<Vec<Edit>> {
    let mut edits = Vec::new();
    for (path, change) in &cv.file_changes {
        if !keep(path) {
            continue;
        }
        edits.push(match change {
            FileChange::Set { content, kind, .. } => Edit::Stored {
                path: path.clone(),
                content: content.clone(),
                kind: *kind,
            },
            FileChange::Delete => Edit::Delete { path: path.clone() },
            FileChange::Conflict(_) => {
                return Err(CliError::conflict(format!(
                    "{path} has unresolved conflicts; resolve them first"
                )))
            }
        });
    }
    Ok(edits)
}

fn root_of(parent: Option<&ChangeVersionHash>) -> Root {
    match parent {
        Some(p) => Root::Version(*p),
        None => Root::Tree(Tree::empty_hash()),
    }
}

/// Move the checkout's base onto `onto`, keeping its files, with
/// `working` as the working change.
async fn rebase_checkout(
    ctx: &mut Ctx<'_>,
    onto: &Root,
    working: rill_types::ChangeId,
) -> Result<()> {
    let mount = ctx.mount().await?;
    ctx.daemon()
        .await?
        .update_mount(dpb::UpdateMountRequest {
            name: mount.name,
            revision: onto.to_string(),
            keep_files: true,
            working_change: working.to_string(),
        })
        .await?;
    Ok(())
}

/// Run `rill squash`: fold the working change into the unlanded change
/// under it, which becomes the working change.
pub async fn squash(ctx: &mut Ctx<'_>, args: SquashArgs) -> Result<()> {
    let repo = ctx.repo()?;
    if ctx.working_change().await?.is_none() {
        return Err(CliError::usage("no working change to squash"));
    }
    let (top, _) = commit(ctx, None, None, false).await?;
    let parent = top
        .version
        .parents
        .first()
        .copied()
        .ok_or_else(|| CliError::usage("the working change has no parent to squash into"))?;
    let under = repo.change_info(parent).await?;
    if under.trunk_seq.is_some() {
        return Err(CliError::usage(
            "the change under the working change has landed; nothing to squash into",
        ));
    }
    let edits = edits_of(&top.version, |_| true)?;
    let message = args.message.unwrap_or_else(|| under.message.clone());
    let mut opts = ApplyOptions::message(message).with_provenance(ctx.provenance());
    opts.change_id = Some(under.change_id);
    let combined = repo
        .apply_edits(EditBase::Change(under.change_id), edits, opts)
        .await?;
    rebase_checkout(
        ctx,
        &root_of(combined.version.parents.first()),
        under.change_id,
    )
    .await?;
    if let Err(e) = repo
        .reviews()
        .abandon(
            top.version.change_id,
            Some("squashed into its parent".into()),
        )
        .await
    {
        tracing::debug!(error = %e, "the squashed change had no review to abandon");
    }
    let squashed = top.version.change_id;
    let mut value = json::version_result(&combined);
    value["squashed"] = j!(squashed.to_string());
    ctx.emit(value, |out| {
        print_version(out, "squashed into", &combined)?;
        let painted = out.paint(Role::Dim, &format!("{squashed} is abandoned"));
        out.line(painted)
    })
}

/// Run `rill split`: move the named paths' edits into a new change under
/// the working change.
pub async fn split(ctx: &mut Ctx<'_>, args: SplitArgs) -> Result<()> {
    let repo = ctx.repo()?;
    if ctx.working_change().await?.is_none() {
        return Err(CliError::usage("no working change to split"));
    }
    let prefixes: Vec<RepoPathBuf> = args
        .paths
        .iter()
        .map(RepoPathBuf::new)
        .collect::<std::result::Result<_, _>>()?;
    let picked = |p: &RepoPathBuf| prefixes.iter().any(|x| p.starts_with(x));
    let (top, _) = commit(ctx, None, None, false).await?;
    let first = edits_of(&top.version, picked)?;
    let rest = edits_of(&top.version, |p| !picked(p))?;
    if first.is_empty() {
        return Err(CliError::usage("no edits under those paths"));
    }
    if rest.is_empty() {
        return Err(CliError::usage(
            "every edit is under those paths; nothing would be left",
        ));
    }
    let base = root_of(top.version.parents.first());
    let summary = top.version.message.lines().next().unwrap_or("").to_string();
    let message = args
        .message
        .unwrap_or_else(|| format!("Split from: {summary}"));
    let lower = repo
        .apply_edits(
            EditBase::Root(base),
            first,
            ApplyOptions::message(message).with_provenance(ctx.provenance()),
        )
        .await?;
    let mut opts =
        ApplyOptions::message(top.version.message.clone()).with_provenance(ctx.provenance());
    opts.change_id = Some(top.version.change_id);
    let upper: VersionResult = repo
        .apply_edits(EditBase::Root(Root::Version(lower.hash)), rest, opts)
        .await?;
    rebase_checkout(ctx, &Root::Version(lower.hash), top.version.change_id).await?;
    ctx.emit(
        j!({ "lower": json::version_result(&lower), "upper": json::version_result(&upper) }),
        |out| {
            print_version(out, "split off", &lower)?;
            print_version(out, "kept", &upper)
        },
    )
}

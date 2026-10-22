//! Revision strings: what a request names as a point in a repository's
//! history.
//!
//! `trunk` (also the empty string), `trunk@N`, a 64-digit version hash,
//! `tree:<hex>` (a bare tree, with no version), `changes/{change}` (its
//! current version), `workspaces/{workspace}` (its snapshot, or its base
//! before one), or any other ref name.

use rill_core::RootTree;
use rill_types::{ChangeId, ChangeVersionHash, RefName, TreeHash, TrunkSeq, WorkspaceId};

use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

/// A revision resolved to its root tree, with the version when it names one.
#[derive(Debug, Clone, Copy)]
pub struct Resolved {
    pub version: Option<ChangeVersionHash>,
    pub root: TreeHash,
}

/// What `revision` names in `ctx`. An empty repository's trunk is the
/// empty tree with no version.
pub async fn resolve(ctx: &RepoContext, revision: &str) -> Result<Resolved> {
    let bad = |why: &str| Error::InvalidRevision(format!("{revision:?}: {why}"));
    let version = match revision {
        "" | "trunk" => ctx.repo.trunk_head().await?.map(|(_, h)| h),
        r if r.starts_with("trunk@") => {
            let seq: u64 = r["trunk@".len()..]
                .parse()
                .map_err(|_| bad("trunk@ takes a position"))?;
            Some(
                ctx.repo
                    .trunk_at(TrunkSeq(seq))
                    .await?
                    .ok_or_else(|| Error::not_found(format!("trunk@{seq}")))?,
            )
        }
        r if r.starts_with("tree:") => {
            let root: TreeHash = r["tree:".len()..]
                .parse()
                .map_err(|_| bad("tree: takes a hash"))?;
            ctx.trees.get_tree(&root).await?;
            return Ok(Resolved {
                version: None,
                root,
            });
        }
        r if r.starts_with("changes/") => {
            let id: ChangeId = r["changes/".len()..]
                .parse()
                .map_err(|_| bad("changes/ takes a change id"))?;
            let review = ctx.repo.get_review(&id).await?;
            match review.as_ref().and_then(|r| r.current_version()) {
                Some(v) => Some(v),
                None => {
                    let versions = ctx.repo.change_versions(&id).await?;
                    Some(
                        *versions
                            .last()
                            .ok_or_else(|| Error::not_found(format!("change {id}")))?,
                    )
                }
            }
        }
        r if r.starts_with("workspaces/") => {
            let id: WorkspaceId = r["workspaces/".len()..]
                .parse()
                .map_err(|_| bad("workspaces/ takes a workspace id"))?;
            let ws = ctx
                .repo
                .get_workspace(&id)
                .await?
                .ok_or_else(|| Error::not_found(format!("workspace {id}")))?;
            if let Some(root) = ws.snapshot {
                return Ok(Resolved {
                    version: None,
                    root,
                });
            }
            Some(ws.base)
        }
        r if r.len() == 64 && r.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Some(r.parse().map_err(|_| bad("not a version hash"))?)
        }
        r => {
            let name = RefName::new(r).map_err(|e| bad(&e.to_string()))?;
            Some(
                ctx.repo
                    .get_ref(&name)
                    .await?
                    .ok_or_else(|| Error::not_found(format!("ref {name}")))?,
            )
        }
    };
    let root = match version {
        Some(v) => ctx.derive.get::<RootTree>(&v).await?,
        None => ctx.trees.empty_tree().await?,
    };
    Ok(Resolved { version, root })
}

/// What `revision` names, which must be a version (not a bare tree or an
/// empty trunk).
pub async fn resolve_version(ctx: &RepoContext, revision: &str) -> Result<ChangeVersionHash> {
    resolve(ctx, revision)
        .await?
        .version
        .ok_or_else(|| Error::InvalidRevision(format!("{revision:?} does not name a version")))
}

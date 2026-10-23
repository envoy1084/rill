//! Moving a mount onto a new base. Each path is classified by what
//! changed upstream (old base → new base) and locally (old base →
//! overlay). One side changed: take it. Both: three-way merge; a clean
//! merge stays a local edit, a conflicted one is written with markers and
//! flagged until resolved. Only paths changed on either side are visited.

use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use rill_sdk::Root;
use rill_types::{ChangeVersionHash, ContentRef, FileKind, RepoPathBuf, Tree, TreeHash};

use super::merge::{conflict_text, merge_texts, Merged};
use super::mount::{Change, Mount};
use super::record::MountMode;
use crate::eager::{apply_disk, DiskOp};
use crate::error::Result;
use crate::overlay::{classify, ChangeKind, OverlayOp};
use crate::view::diff_trees;

/// What an update did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateReport {
    /// Files changed upstream and taken as they are.
    pub updated: Vec<RepoPathBuf>,
    /// Files removed upstream.
    pub removed: Vec<RepoPathBuf>,
    /// Files whose local edits merged cleanly with upstream.
    pub merged: Vec<RepoPathBuf>,
    /// Files written with conflict markers.
    pub conflicts: Vec<RepoPathBuf>,
}

/// The change version and root tree `root` names. An empty repository's
/// trunk is the empty tree with no version.
pub(crate) async fn resolve_base(
    repo: &rill_sdk::RepoClient,
    root: &Root,
) -> Result<(Option<ChangeVersionHash>, TreeHash)> {
    if let Root::Tree(tree) = root {
        return Ok((None, *tree));
    }
    if *root == Root::trunk() {
        let empty = match repo.trunk_head().await {
            Ok(head) => head.is_none(),
            Err(e) if e.is_not_found() => true,
            Err(e) => return Err(e.into()),
        };
        if empty {
            return Ok((None, Tree::empty_hash()));
        }
    }
    let version = repo.resolve_root(root).await?;
    let listing = repo
        .list_dir_full(&Root::Version(version), &RepoPathBuf::root())
        .await?;
    Ok((Some(version), listing.tree_hash))
}

impl Mount {
    /// Move onto `onto`, merging local edits, and forget the working
    /// change once it has landed so the next commit starts a new one.
    pub async fn update(&self, onto: Root) -> Result<UpdateReport> {
        self.settle().await?;
        let _op = self.ops.lock().await;
        let report = self.update_locked(onto).await?;
        if let Some(id) = self.record().working_change {
            let landed = match self.repo.current_version(id).await {
                Ok(info) => info.trunk_seq.is_some(),
                Err(e) if e.is_not_found() => true,
                Err(e) => return Err(e.into()),
            };
            if landed {
                let mut record = self.record();
                record.working_change = None;
                self.write(
                    Change {
                        record: Some(record),
                        ..Change::default()
                    },
                    None,
                )
                .await?;
            }
        }
        Ok(report)
    }

    pub(crate) async fn update_locked(&self, onto: Root) -> Result<UpdateReport> {
        let _quiet = self.quiesce().await;
        let mut record = self.record();
        let (version, tree) = resolve_base(&self.repo, &onto).await?;
        if let Some(ws) = record.workspace {
            // The local update must not fail because the server workspace
            // could not follow; the next push rebuilds its snapshot anyway.
            if let Err(e) = self
                .repo
                .workspaces()
                .update_base(ws, version.map(Root::Version))
                .await
            {
                tracing::warn!(workspace = %ws, error = %e, "workspace base not moved");
            }
        }
        let old = self.base();
        let upstream: BTreeMap<_, _> = diff_trees(&old, old.root(), tree, &record.sparse)
            .await?
            .into_iter()
            .map(|c| (c.path.clone(), c))
            .collect();
        let local: BTreeMap<_, _> = self.entries().into_iter().collect();
        let paths: BTreeSet<RepoPathBuf> = upstream.keys().chain(local.keys()).cloned().collect();
        let eager = record.mode == MountMode::Eager;
        let mut staged = Staged {
            eager,
            ..Staged::default()
        };
        let mut report = UpdateReport::default();

        for path in paths {
            let up = upstream.get(&path);
            let entry = local.get(&path);
            let old_file = self.base_file_in(&old, &path).await?;
            let kind = entry.and_then(|e| classify(e, old_file.as_ref()));
            match (kind, up) {
                (None, None) => {
                    if entry.is_some() {
                        staged.change.overlay.push((path, None));
                    }
                }
                (None, Some(up)) => {
                    if entry.is_some() {
                        staged.change.overlay.push((path.clone(), None));
                    }
                    match &up.new {
                        Some((content, kind)) => {
                            if eager {
                                let bytes = self.bytes(content).await?;
                                staged.disk.push(DiskOp::Write {
                                    path: path.clone(),
                                    bytes,
                                    kind: *kind,
                                });
                            }
                            report.updated.push(path);
                        }
                        None => {
                            if eager {
                                staged.disk.push(DiskOp::Remove { path: path.clone() });
                            }
                            report.removed.push(path);
                        }
                    }
                }
                (Some(_), None) | (Some(ChangeKind::Conflict), Some(_)) => {}
                (Some(ChangeKind::Deleted), Some(up)) => match &up.new {
                    None => staged.change.overlay.push((path, None)),
                    Some((content, kind)) => {
                        let base = self.bytes_of(&up.old).await?;
                        let merged = conflict_text(base, None, Some(self.bytes(content).await?));
                        self.stage(&mut staged, path.clone(), merged, *kind).await?;
                        report.conflicts.push(path);
                    }
                },
                (Some(_), Some(up)) => {
                    let Some(OverlayOp::Set {
                        digest,
                        kind: local_kind,
                        ..
                    }) = entry.map(|e| e.op)
                    else {
                        continue;
                    };
                    let local_bytes = self.shared.content.local(digest).await?;
                    let (merged, kind) = match (&up.old, &up.new) {
                        (Some((old, _)), Some((new, new_kind))) => {
                            let base = self.bytes(old).await?;
                            let upstream = self.bytes(new).await?;
                            (
                                merge_texts(&path, base, local_bytes, upstream).await,
                                *new_kind,
                            )
                        }
                        (Some((old, _)), None) => {
                            let base = self.bytes(old).await?;
                            (
                                conflict_text(Some(base), Some(local_bytes), None),
                                local_kind,
                            )
                        }
                        (None, Some((new, new_kind))) => {
                            let upstream = self.bytes(new).await?;
                            if upstream == local_bytes && local_kind == *new_kind {
                                staged.change.overlay.push((path, None));
                                continue;
                            }
                            (
                                merge_texts(&path, Bytes::new(), local_bytes, upstream).await,
                                *new_kind,
                            )
                        }
                        (None, None) => continue,
                    };
                    let conflict = merged.conflict;
                    self.stage(&mut staged, path.clone(), merged, kind).await?;
                    if conflict {
                        report.conflicts.push(path);
                    } else {
                        report.merged.push(path);
                    }
                }
            }
        }

        record.base = onto.to_string();
        record.base_version = version;
        record.base_tree = tree;
        record.etag = None;
        staged.change.record = Some(record.clone());
        self.write(staged.change, Some(tree)).await?;
        if !staged.disk.is_empty() {
            let files = apply_disk(&record.path, staged.disk).await?;
            self.write(
                Change {
                    files,
                    ..Change::default()
                },
                None,
            )
            .await?;
        }
        Ok(report)
    }

    async fn bytes(&self, content: &ContentRef) -> Result<Bytes> {
        self.shared.content.get(&self.repo, content).await
    }

    async fn bytes_of(&self, file: &Option<(ContentRef, FileKind)>) -> Result<Option<Bytes>> {
        Ok(match file {
            Some((content, _)) => Some(self.bytes(content).await?),
            None => None,
        })
    }

    async fn stage(
        &self,
        staged: &mut Staged,
        path: RepoPathBuf,
        merged: Merged,
        kind: FileKind,
    ) -> Result<()> {
        let size = merged.text.len() as u64;
        let digest = self.shared.content.put_local(merged.text.clone()).await?;
        staged.change.overlay.push((
            path.clone(),
            Some((OverlayOp::Set { digest, kind, size }, merged.conflict)),
        ));
        if staged.eager {
            staged.disk.push(DiskOp::Write {
                path,
                bytes: merged.text,
                kind,
            });
        }
        Ok(())
    }
}

#[derive(Default)]
struct Staged {
    eager: bool,
    change: Change,
    disk: Vec<DiskOp>,
}

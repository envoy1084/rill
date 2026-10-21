//! Moving a checkout to a new root while preserving local modifications.
//!
//! Each path is classified by what happened upstream (between the old base
//! and the new root) and locally (between the old base and the working
//! copy). One side changed: take that side. Both changed: three-way merge
//! with the old base content; a clean merge is written as a local
//! modification, a conflicted one as marker text flagged `Conflict`.
//!
//! A merged or conflicted file's manifest entry records the *upstream*
//! content as its comparison base, so status keeps reporting it as
//! modified until the working copy matches the new root.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use bytes::Bytes;
use std::sync::{Arc, OnceLock};

use rill_core::merge::{
    materialize as render_conflict, merge_file, ContentMerger, FileMergeResult, Structural,
};
use rill_types::{FileKind, Merge, ObjectHash, RepoPath, RepoPathBuf};

use super::checkout::{materialize, resolve_version, Checkout};
use super::fs::{local_hash, prune_empty_dirs, read_entry, remove_entry, stat, write_entry};
use super::meta::{Manifest, ManifestEntry};
use super::remote::{self, RemoteFile};
use super::status::LocalChangeKind;
use super::walk::to_fs_path;
use crate::error::{Error, Result};
use crate::types::Root;

/// What an update did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UpdateReport {
    /// Files rewritten from the new root.
    pub updated: Vec<RepoPathBuf>,
    /// Files removed because the new root lacks them.
    pub removed: Vec<RepoPathBuf>,
    /// Files whose local modifications merged cleanly with upstream.
    pub merged: Vec<RepoPathBuf>,
    /// Files written with conflict markers.
    pub conflicts: Vec<RepoPathBuf>,
}

impl Checkout {
    /// Re-materialize the working copy at `new_root`, keeping local
    /// modifications and merging them where upstream changed too.
    pub async fn update(&mut self, new_root: Root) -> Result<UpdateReport> {
        let new_files = remote::list_tree(self.repo(), &new_root, &self.meta().sparse).await?;
        let local: BTreeMap<RepoPathBuf, LocalChangeKind> = self
            .status()
            .await?
            .into_iter()
            .map(|c| (c.path, c.kind))
            .collect();
        let old_manifest = self.manifest().clone();
        let read_root = self.read_root();

        let paths: BTreeSet<&RepoPathBuf> = old_manifest.keys().chain(new_files.keys()).collect();
        let mut manifest = Manifest::new();
        let mut report = UpdateReport::default();

        for path in paths {
            let old = old_manifest.get(path);
            let new = new_files.get(path);
            let change = local.get(path).copied();
            let outcome = match (old, new) {
                (Some(old), Some(new)) => {
                    let upstream_changed =
                        new.content.content_hash() != old.content_hash || new.kind != old.kind;
                    match (change, upstream_changed) {
                        (_, false) | (Some(LocalChangeKind::Conflict), true) => {
                            Outcome::Keep(old.clone())
                        }
                        (None, true) => Outcome::Fetch,
                        (Some(LocalChangeKind::Deleted), true) => {
                            self.merge_with_upstream(&read_root, path, None, new)
                                .await?
                        }
                        (Some(_), true) => {
                            let (bytes, _) = self.read_local(path).await?;
                            self.merge_with_upstream(&read_root, path, Some(bytes), new)
                                .await?
                        }
                    }
                }
                (Some(old), None) => match change {
                    None => Outcome::Remove,
                    Some(LocalChangeKind::Deleted) => Outcome::Drop,
                    Some(LocalChangeKind::Conflict) => Outcome::Keep(old.clone()),
                    Some(_) => {
                        let (bytes, kind) = self.read_local(path).await?;
                        let base = self.repo().read_file(&read_root, path).await?;
                        let terms = Merge::three_way(Some(base), Some(bytes), None);
                        Outcome::Write(Written {
                            text: Bytes::from(render_conflict(&terms)),
                            kind,
                            content_hash: old.content_hash,
                            upstream: Bytes::new(),
                            conflict: true,
                        })
                    }
                },
                (None, Some(new)) => match change {
                    Some(LocalChangeKind::Added) => {
                        let (bytes, kind) = self.read_local(path).await?;
                        let upstream = remote::fetch(self.repo(), new).await?;
                        if bytes == upstream && kind == new.kind {
                            Outcome::Adopt(new.clone())
                        } else {
                            Outcome::Write(merge_bytes(path, &[], &bytes, upstream, new).await)
                        }
                    }
                    _ => Outcome::Fetch,
                },
                (None, None) => continue,
            };
            self.apply_outcome(path, new, outcome, &mut manifest, &mut report)
                .await?;
        }

        let base_version = resolve_version(self.repo(), &new_root).await?;
        let meta = self.meta_mut();
        meta.base = new_root;
        meta.base_version = base_version;
        *self.manifest_mut() = manifest;
        self.save().await?;
        Ok(report)
    }

    /// The root to read base content from: the pinned version when the
    /// base had one, since a ref may have moved since the checkout.
    fn read_root(&self) -> Root {
        match self.meta().base_version {
            Some(h) => Root::Version(h),
            None => self.meta().base.clone(),
        }
    }

    async fn read_local(&self, path: &RepoPath) -> Result<(Bytes, FileKind)> {
        let fs_path = self.fs_path(path);
        tokio::task::spawn_blocking(move || read_entry(&fs_path))
            .await
            .map_err(|e| Error::checkout(format!("read task failed: {e}")))?
    }

    /// Three-way merge of the local content (`None` when deleted locally)
    /// with the upstream file against the old base.
    async fn merge_with_upstream(
        &self,
        read_root: &Root,
        path: &RepoPath,
        local: Option<Bytes>,
        new: &RemoteFile,
    ) -> Result<Outcome> {
        let base = self.repo().read_file(read_root, path).await?;
        let upstream = remote::fetch(self.repo(), new).await?;
        Ok(Outcome::Write(match local {
            Some(local) => merge_bytes(path, &base, &local, upstream, new).await,
            None => {
                let terms = Merge::three_way(Some(base), None, Some(upstream.clone()));
                Written {
                    text: Bytes::from(render_conflict(&terms)),
                    kind: new.kind,
                    content_hash: new.content.content_hash(),
                    upstream,
                    conflict: true,
                }
            }
        }))
    }

    async fn apply_outcome(
        &self,
        path: &RepoPathBuf,
        new: Option<&RemoteFile>,
        outcome: Outcome,
        manifest: &mut Manifest,
        report: &mut UpdateReport,
    ) -> Result<()> {
        let root = self.path().to_path_buf();
        match outcome {
            Outcome::Keep(entry) => {
                manifest.insert(path.clone(), entry);
            }
            Outcome::Drop => {}
            Outcome::Remove => {
                let fs_path = to_fs_path(&root, path);
                tokio::task::spawn_blocking(move || {
                    remove_entry(&fs_path)?;
                    prune_empty_dirs(&root, &fs_path);
                    Ok::<_, Error>(())
                })
                .await
                .map_err(|e| Error::checkout(format!("remove task failed: {e}")))??;
                report.removed.push(path.clone());
            }
            Outcome::Fetch => {
                let file = new.ok_or_else(|| Error::checkout("fetch without a remote file"))?;
                let entry = materialize(self.repo(), &root, path, file).await?;
                manifest.insert(path.clone(), entry);
                report.updated.push(path.clone());
            }
            Outcome::Adopt(file) => {
                let fs_path = to_fs_path(&root, path);
                let entry = tokio::task::spawn_blocking(move || {
                    let st = stat(&fs_path)?.ok_or_else(|| {
                        Error::checkout(format!("{} vanished during update", fs_path.display()))
                    })?;
                    let (bytes, kind) = read_entry(&fs_path)?;
                    Ok::<_, Error>(ManifestEntry {
                        content_hash: file.content.content_hash(),
                        local_hash: local_hash(&bytes),
                        size: st.size,
                        mtime_ms: st.mtime_ms,
                        kind,
                        conflict: false,
                    })
                })
                .await
                .map_err(|e| Error::checkout(format!("adopt task failed: {e}")))??;
                manifest.insert(path.clone(), entry);
            }
            Outcome::Write(written) => {
                let conflict = written.conflict;
                let entry = write_local(&root, path, written).await?;
                manifest.insert(path.clone(), entry);
                if conflict {
                    report.conflicts.push(path.clone());
                } else {
                    report.merged.push(path.clone());
                }
            }
        }
        Ok(())
    }
}

/// Content produced by a merge, to be written and tracked against the
/// upstream bytes it was merged with.
struct Written {
    text: Bytes,
    kind: FileKind,
    content_hash: ObjectHash,
    upstream: Bytes,
    conflict: bool,
}

/// What to do with one path.
enum Outcome {
    /// Keep the old manifest entry; the file is untouched.
    Keep(ManifestEntry),
    /// Forget the path; nothing on disk to change.
    Drop,
    /// Delete the file.
    Remove,
    /// Fetch the new content and write it.
    Fetch,
    /// The local file already matches upstream; just track it.
    Adopt(RemoteFile),
    /// Write merged or conflicted content.
    Write(Written),
}

/// The structural merger sync uses, unless `RILL_STRUCTURAL_MERGE` is
/// `false`.
fn structural_merger() -> Option<&'static Arc<dyn ContentMerger>> {
    static MERGER: OnceLock<Option<Arc<dyn ContentMerger>>> = OnceLock::new();
    MERGER
        .get_or_init(|| {
            let off = std::env::var("RILL_STRUCTURAL_MERGE")
                .is_ok_and(|v| matches!(v.as_str(), "0" | "false" | "no" | "off"));
            (!off).then(|| {
                Arc::new(rill_merge_syntax::WeaveMerger::new(&Default::default()))
                    as Arc<dyn ContentMerger>
            })
        })
        .as_ref()
}

async fn merge_bytes(
    path: &RepoPath,
    base: &[u8],
    local: &[u8],
    upstream: Bytes,
    new: &RemoteFile,
) -> Written {
    let mut merged = merge_file(base, local, &upstream);
    if let (FileMergeResult::Conflicted { .. }, Some(merger)) = (&merged, structural_merger()) {
        let structural = Structural { merger, path };
        if let Some(bytes) = structural
            .resolve(
                Bytes::copy_from_slice(base),
                Bytes::copy_from_slice(local),
                upstream.clone(),
            )
            .await
        {
            merged = FileMergeResult::Resolved(bytes);
        }
    }
    let (text, conflict) = match merged {
        FileMergeResult::Resolved(bytes) => (Bytes::from(bytes), false),
        FileMergeResult::Conflicted { .. } => {
            let terms = Merge::three_way(
                Some(Bytes::copy_from_slice(base)),
                Some(Bytes::copy_from_slice(local)),
                Some(upstream.clone()),
            );
            (Bytes::from(render_conflict(&terms)), true)
        }
    };
    Written {
        text,
        kind: new.kind,
        content_hash: new.content.content_hash(),
        upstream,
        conflict,
    }
}

/// Write merged content. The entry's comparison fields describe the
/// upstream bytes, with an mtime that never matches so status always
/// hashes the file.
async fn write_local(root: &Path, path: &RepoPath, written: Written) -> Result<ManifestEntry> {
    let fs_path = to_fs_path(root, path);
    tokio::task::spawn_blocking(move || {
        write_entry(&fs_path, &written.text, written.kind)?;
        Ok(ManifestEntry {
            content_hash: written.content_hash,
            local_hash: local_hash(&written.upstream),
            size: written.upstream.len() as u64,
            mtime_ms: -1,
            kind: written.kind,
            conflict: written.conflict,
        })
    })
    .await
    .map_err(|e| Error::checkout(format!("write task failed: {e}")))?
}

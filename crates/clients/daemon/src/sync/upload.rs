//! Overlay entries as edits whose content is on the server: small files
//! inline, the rest uploaded unless the server already has them.

use futures::{StreamExt, TryStreamExt};
use rill_sdk::Edit;
use rill_types::objects::blob::CHUNK_THRESHOLD;
use rill_types::{ContentRef, ObjectKind, RepoPathBuf, INLINE_THRESHOLD};

use crate::error::Result;
use crate::mount::Mount;
use crate::overlay::{OverlayEntry, OverlayOp};

/// Uploads in flight at once.
const UPLOADS: usize = 16;
/// Objects per existence check.
const CHECK_BATCH: usize = 1000;

/// `entries` as edits with stored content.
pub(crate) async fn stored_edits(
    mount: &Mount,
    entries: &[(RepoPathBuf, OverlayEntry)],
) -> Result<Vec<Edit>> {
    let content = &mount.shared.content;
    let mut probe = Vec::new();
    for (_, entry) in entries {
        if let OverlayOp::Set { digest, size, .. } = entry.op {
            let small = size as usize <= INLINE_THRESHOLD;
            if !small && size < CHUNK_THRESHOLD && !mount.uploaded.contains_key(&digest) {
                probe.push((digest, size));
            }
        }
    }
    probe.sort();
    probe.dedup();
    for batch in probe.chunks(CHECK_BATCH) {
        let objects: Vec<_> = batch.iter().map(|(d, _)| (ObjectKind::Blob, *d)).collect();
        let present = mount.repo.has_objects(&objects).await?;
        for ((digest, size), present) in batch.iter().zip(present) {
            if present {
                let blob = ContentRef::Blob {
                    hash: *digest,
                    size: *size,
                };
                mount.uploaded.insert(*digest, blob);
            }
        }
    }
    futures::stream::iter(entries.iter().cloned())
        .map(|(path, entry)| async move {
            Ok(match entry.op {
                OverlayOp::Delete => Edit::Delete { path },
                OverlayOp::Set { digest, kind, size } => {
                    let stored = match mount.uploaded.get(&digest) {
                        Some(c) => c,
                        None => {
                            let bytes = content.local(digest).await?;
                            let stored = if size as usize <= INLINE_THRESHOLD {
                                ContentRef::Inline { bytes }
                            } else {
                                mount.repo.put_blob(bytes).await?
                            };
                            if !stored.is_inline() {
                                mount.uploaded.insert(digest, stored.clone());
                            }
                            stored
                        }
                    };
                    Edit::Stored {
                        path,
                        content: stored,
                        kind,
                    }
                }
            })
        })
        .buffered(UPLOADS)
        .try_collect()
        .await
}

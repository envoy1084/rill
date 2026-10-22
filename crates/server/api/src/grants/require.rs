//! Checking that a fetch by hash names an object its caller was granted.

use rill_policy::grants::ReadGrant;
use rill_store::ObjectStoreExt;
use rill_types::{BlobManifest, ObjectHash, ObjectKind, RepoPathBuf, Timestamp};

use crate::services::common::Opened;
use rill_runtime::error::{Error, Result};

fn verified(opened: &Opened, grant: Option<&str>) -> Result<ReadGrant> {
    let text = grant
        .filter(|g| !g.is_empty())
        .ok_or_else(|| Error::denied("fetching an object by hash needs a read grant"))?;
    let grant = opened
        .repo
        .identity
        .grant_keys()
        .verify(text, Timestamp::now())
        .map_err(|e| Error::denied(e.to_string()))?;
    if grant.repo != opened.repo.id() || grant.principal != opened.ctx.principal.id {
        return Err(Error::denied(
            "the read grant is for another repository or caller",
        ));
    }
    Ok(grant)
}

/// The path through which the caller was granted `(kind, hash)`; `None` in
/// dev mode, where everything is readable.
pub fn require(
    opened: &Opened,
    grant: Option<&str>,
    kind: ObjectKind,
    hash: &ObjectHash,
) -> Result<Option<RepoPathBuf>> {
    if opened.dev_mode() {
        return Ok(None);
    }
    let grant = verified(opened, grant)?;
    let entry = grant
        .entry(kind, hash)
        .ok_or_else(|| Error::denied(format!("the read grant does not name {kind} {hash}")))?;
    Ok(Some(entry.path.clone()))
}

/// A chunk is granted through the manifest that lists it.
pub async fn require_chunk(
    opened: &Opened,
    grant: Option<&str>,
    chunk: &ObjectHash,
    manifest: Option<&ObjectHash>,
) -> Result<()> {
    if opened.dev_mode() {
        return Ok(());
    }
    let manifest =
        manifest.ok_or_else(|| Error::denied("a chunk is fetched through its manifest (`via`)"))?;
    require(opened, grant, ObjectKind::BlobManifest, manifest)?;
    let listed: BlobManifest = opened
        .repo
        .repo
        .objects()
        .get_object_required(manifest)
        .await?;
    if listed.chunks.iter().any(|c| c.hash == *chunk) {
        Ok(())
    } else {
        Err(Error::denied(format!(
            "manifest {manifest} does not list chunk {chunk}"
        )))
    }
}

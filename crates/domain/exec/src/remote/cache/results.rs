//! The action cache: an action's digest to the result of running it.
//!
//! Entries are sealed under the org's data keys and bound to the action,
//! so an entry written into the metadata store by anything but the
//! scheduler, or moved to another action, does not open. A result is
//! cached only when everything it refers to is stored, and is served only
//! while it still is.

use bytes::Bytes;
use prost::Message;
use rill_reapi_proto::reapi::{ActionResult, Digest};

use crate::error::Result;
use crate::remote::{BlobDigest, ExecSpace, Scheduler};

const RESULT_KIND: &str = "reapi:ac";

/// The namespace whose data keys seal `org`'s action cache.
fn namespace(org: &str) -> String {
    format!("{org}.ac")
}

/// Every blob `result` refers to.
fn referenced(result: &ActionResult) -> Vec<BlobDigest> {
    let files = result.output_files.iter().filter_map(|f| f.digest.as_ref());
    let trees = result
        .output_directories
        .iter()
        .filter_map(|d| d.tree_digest.as_ref());
    let logs = [result.stdout_digest.as_ref(), result.stderr_digest.as_ref()]
        .into_iter()
        .flatten();
    files
        .chain(trees)
        .chain(logs)
        .filter_map(|d: &Digest| BlobDigest::parse(d).ok())
        .collect()
}

impl Scheduler {
    /// The cached result of `action`, if there is one whose outputs are
    /// all still stored.
    pub async fn cached_result(
        &self,
        space: &ExecSpace,
        action: &BlobDigest,
    ) -> Result<Option<ActionResult>> {
        let Some(sealed) = space.records.get_derived(RESULT_KIND, &action.hash).await? else {
            return Ok(None);
        };
        let plain = match self
            .open(&namespace(&space.org), &action.hash, sealed.to_vec())
            .await
        {
            Ok(plain) => plain,
            Err(e) => {
                tracing::warn!(%action, error = %e, "an action cache entry does not open; ignoring it");
                return Ok(None);
            }
        };
        let Ok(result) = ActionResult::decode(Bytes::from(plain)) else {
            tracing::warn!(%action, "an action cache entry does not decode; ignoring it");
            return Ok(None);
        };
        if !space.missing(&referenced(&result)).await?.is_empty() {
            return Ok(None);
        }
        Ok(Some(result))
    }

    /// Cache `result` for `action`, unless it refers to content that is
    /// not stored; whether it was cached.
    pub async fn cache_result(
        &self,
        space: &ExecSpace,
        action: &BlobDigest,
        result: &ActionResult,
    ) -> Result<bool> {
        let missing = space.missing(&referenced(result)).await?;
        if !missing.is_empty() {
            tracing::warn!(%action, missing = missing.len(), "not caching a result whose outputs are not stored");
            return Ok(false);
        }
        let sealed = self
            .seal(
                &namespace(&space.org),
                &action.hash,
                &result.encode_to_vec(),
            )
            .await?;
        space
            .records
            .put_derived(RESULT_KIND, &action.hash, Bytes::from(sealed))
            .await?;
        Ok(true)
    }

    /// Forget the cached result of `action`.
    pub async fn evict_result(&self, space: &ExecSpace, action: &BlobDigest) -> Result<()> {
        space
            .records
            .delete_derived(RESULT_KIND, &action.hash)
            .await?;
        Ok(())
    }
}

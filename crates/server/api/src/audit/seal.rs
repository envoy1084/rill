//! Sealing an org's chain: its new entries exported as a blob in the org's
//! object store, and a root over them signed with the audit key.

use bytes::Bytes;
use rill_store::ObjectStoreExt;
use rill_types::audit::AuditRoot;
use rill_types::{ObjectHash, Timestamp};

use crate::error::Result;
use crate::state::AppState;

const PAGE: usize = 1000;

/// Seal what `org`'s chain gained since its last root. `None` when it
/// gained nothing, or another server sealed the same entries first.
pub async fn seal(state: &AppState, org: &str) -> Result<Option<AuditRoot>> {
    let audit = state.stores.audit();
    let first = audit.latest_root(org).await?.map_or(1, |r| r.last_seq + 1);
    let head = audit.head(org).await?;
    if head.seq < first {
        return Ok(None);
    }
    let mut lines = Vec::new();
    let mut after = first - 1;
    while after < head.seq {
        let page = audit.entries(org, after, PAGE).await?;
        let Some(last) = page.last() else { break };
        after = last.seq;
        for entry in page.into_iter().filter(|e| e.seq <= head.seq) {
            lines.extend(
                serde_json::to_vec(&entry)
                    .map_err(|e| crate::error::Error::internal(e.to_string()))?,
            );
            lines.push(b'\n');
        }
    }
    let export: ObjectHash = state
        .stores
        .org(org)
        .objects
        .put_blob(Bytes::from(lines))
        .await?;
    let signer = state.identity.audit_signer()?;
    let root = signer.sign(AuditRoot {
        org: org.to_string(),
        first_seq: first,
        last_seq: head.seq,
        head: head.hash,
        export,
        sealed_at: Timestamp::now(),
        key_id: 0,
        public_key: Vec::new(),
        signature: Vec::new(),
    });
    match audit.put_root(&root).await {
        Ok(()) => {
            tracing::info!(
                org,
                first_seq = first,
                last_seq = head.seq,
                "audit chain sealed"
            );
            Ok(Some(root))
        }
        Err(e) if e.is_conflict() => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Whether `org`'s chain is due a seal: it grew since its last root, and
/// that root (or, before any, its first entry) is `every_ms` old.
pub async fn due(state: &AppState, org: &str, every_ms: i64) -> Result<bool> {
    let audit = state.stores.audit();
    let head = audit.head(org).await?;
    let root = audit.latest_root(org).await?;
    let sealed = root.as_ref().map_or(0, |r| r.last_seq);
    if head.seq <= sealed {
        return Ok(false);
    }
    let since = match root {
        Some(r) => r.sealed_at,
        None => match audit.entries(org, 0, 1).await?.first() {
            Some(first) => first.record.at,
            None => return Ok(false),
        },
    };
    Ok(Timestamp::now().millis() - since.millis() >= every_ms)
}

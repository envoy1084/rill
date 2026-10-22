//! Audit: an org's chained audit log, its seals, and the keys seals are
//! signed with. Entries and roots are decoded from exactly the bytes the
//! server hashed and signed, so a client can verify them itself.

use std::collections::HashMap;

use rill_proto::rill::v1 as pb;
use rill_types::audit::{AuditEntry, AuditRoot};

use crate::client::{retry_idempotent, Client};
use crate::error::{Error, Result};
use crate::stream::EventStream;

/// An org's audit log (`_instance` for instance-wide actions).
#[derive(Clone, Debug)]
pub struct AuditClient {
    client: Client,
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    postcard::from_bytes(bytes)
        .map_err(|e| Error::Invalid(rill_types::Error::Decode(format!("audit record: {e}"))))
}

impl AuditClient {
    pub(crate) fn new(client: Client) -> Self {
        AuditClient { client }
    }

    fn svc(&self) -> crate::client::AuditSvc {
        self.client.audit_svc()
    }

    /// Up to `limit` entries after `after_seq`, and the chain's length.
    pub async fn entries(
        &self,
        org: &str,
        after_seq: u64,
        limit: u32,
    ) -> Result<(Vec<AuditEntry>, u64)> {
        let resp = retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::ListAuditEntriesRequest {
                org: org.to_string(),
                after_seq,
                limit,
            };
            async move { svc.list_audit_entries(request).await }
        })
        .await?
        .into_owned();
        let entries = resp
            .entries
            .iter()
            .map(|e| decode(&e.encoded))
            .collect::<Result<_>>()?;
        Ok((entries, resp.head_seq))
    }

    /// The whole chain, in order.
    pub async fn all_entries(&self, org: &str) -> Result<Vec<AuditEntry>> {
        let mut out: Vec<AuditEntry> = Vec::new();
        loop {
            let after = out.last().map_or(0, |e| e.seq);
            let (page, head) = self.entries(org, after, 5000).await?;
            let done = page.is_empty() || page.last().map(|e| e.seq) >= Some(head);
            out.extend(page);
            if done {
                return Ok(out);
            }
        }
    }

    /// The chain's seals, oldest first.
    pub async fn roots(&self, org: &str) -> Result<Vec<AuditRoot>> {
        let resp = retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::ListAuditRootsRequest {
                org: org.to_string(),
            };
            async move { svc.list_audit_roots(request).await }
        })
        .await?
        .into_owned();
        resp.roots.iter().map(|r| decode(&r.encoded)).collect()
    }

    /// Seal what the chain gained since its last seal.
    pub async fn seal(&self, org: &str) -> Result<Option<AuditRoot>> {
        let resp = retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::SealAuditRequest {
                org: org.to_string(),
            };
            async move { svc.seal_audit(request).await }
        })
        .await?
        .into_owned();
        resp.root.map(|r| decode(&r.encoded)).transpose()
    }

    /// The published audit keys, by token key id.
    pub async fn keys(&self) -> Result<HashMap<u32, Vec<u8>>> {
        let resp = retry_idempotent(|| {
            let svc = self.svc();
            async move { svc.list_audit_keys(pb::ListAuditKeysRequest {}).await }
        })
        .await?
        .into_owned();
        Ok(resp
            .keys
            .into_iter()
            .map(|k| (k.id, k.public_key.to_vec()))
            .collect())
    }

    /// Entries after `after_seq` as they are chained.
    pub async fn watch(&self, org: &str, after_seq: u64) -> Result<EventStream<AuditEntry>> {
        let stream = self
            .svc()
            .watch_audit(pb::WatchAuditRequest {
                org: org.to_string(),
                after_seq,
            })
            .await?;
        Ok(crate::repo::map_stream(
            stream,
            |e: pb::AuditEntryRecord| decode(&e.encoded),
        ))
    }
}

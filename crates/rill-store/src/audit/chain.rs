//! Giving queued records their place in each org's chain. The head is
//! compare-and-set, so two chainers never fork a chain: the loser's batch
//! conflicts and it reads the new head.

use bytes::Bytes;
use rill_types::audit::{AuditEntry, AuditHead};

use super::store::AuditStore;
use crate::error::Result;
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

/// Records chained per transaction.
pub const CHAIN_BATCH: usize = 256;

fn head_key(org: &str) -> Bytes {
    keys::identity_row(table::AUDIT_HEAD, org)
}

fn entry_key(org: &str, seq: u64) -> Bytes {
    keys::identity_pair(table::AUDIT, org, seq as i64)
}

impl AuditStore {
    /// The end of `org`'s chain; empty before its first entry.
    pub async fn head(&self, org: &str) -> Result<AuditHead> {
        match self.meta.get(&head_key(org)).await? {
            Some(raw) => decode_record(&raw),
            None => Ok(AuditHead::default()),
        }
    }

    /// Chain up to [`CHAIN_BATCH`] of `org`'s queued records. Returns how
    /// many; 0 when there were none or another chainer got there first.
    pub async fn chain(&self, org: &str) -> Result<usize> {
        let raw_head = self.meta.get(&head_key(org)).await?;
        let mut head = match &raw_head {
            Some(raw) => decode_record(raw)?,
            None => AuditHead::default(),
        };
        let pending = self.pending(org, CHAIN_BATCH).await?;
        if pending.is_empty() {
            return Ok(0);
        }
        let mut tx = Transaction::new().expect(head_key(org), raw_head);
        for (key, record) in &pending {
            let entry = AuditEntry::after(&head, record.clone());
            head = AuditHead {
                seq: entry.seq,
                hash: entry.hash(),
            };
            tx = tx
                .expect_present(key.clone())
                .delete(key.clone())
                .set(entry_key(org, entry.seq), encode_record(&entry)?);
        }
        tx = tx.set(head_key(org), encode_record(&head)?);
        match self.meta.transact(tx).await {
            Ok(_) => Ok(pending.len()),
            Err(e) if e.is_conflict() => Ok(0),
            Err(e) => Err(e),
        }
    }

    /// Up to `limit` entries of `org` after `after_seq`, in order.
    pub async fn entries(
        &self,
        org: &str,
        after_seq: u64,
        limit: usize,
    ) -> Result<Vec<AuditEntry>> {
        let range = KeyRange::between(
            entry_key(org, after_seq + 1),
            entry_key(org, i64::MAX as u64),
        )
        .limit(limit);
        self.meta
            .scan(range)
            .await?
            .iter()
            .map(|(_, v)| decode_record(v))
            .collect()
    }
}

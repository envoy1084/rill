//! Records waiting for their place in the chain.

use rill_types::audit::AuditRecord;

use super::store::AuditStore;
use crate::error::Result;
use crate::keys::{self, table};
use crate::meta::{KeyRange, Mutation};
use crate::repo::{decode_record, encode_record};

impl AuditStore {
    /// The mutations queuing `record`, for the transaction that does what
    /// it describes.
    pub fn queue(&self, record: &AuditRecord) -> Result<[Mutation; 2]> {
        Ok([
            Mutation::AppendStamped {
                prefix: keys::identity_row(table::AUDIT_PENDING, record.org.as_str()),
                value: encode_record(record)?,
            },
            Mutation::Add {
                key: keys::identity_table(table::AUDIT_PENDING_COUNT),
                delta: 1,
            },
        ])
    }

    /// Queue `record` on its own (for actions that change nothing, such as
    /// a refusal).
    pub async fn record(&self, record: &AuditRecord) -> Result<()> {
        let mut tx = crate::meta::Transaction::new();
        tx.mutations.extend(self.queue(record)?);
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// Records queued so far, ever.
    pub async fn queued(&self) -> Result<i64> {
        self.meta
            .counter(&keys::identity_table(table::AUDIT_PENDING_COUNT))
            .await
    }

    /// Wait until more records are queued than `seen`, or `timeout`.
    pub async fn wait_queued(&self, seen: i64, timeout: std::time::Duration) -> Result<i64> {
        self.meta
            .wait_counter(
                &keys::identity_table(table::AUDIT_PENDING_COUNT),
                seen,
                timeout,
            )
            .await
    }

    /// The orgs with queued records.
    pub async fn pending_orgs(&self) -> Result<Vec<String>> {
        let prefix = keys::identity_table(table::AUDIT_PENDING);
        let mut orgs = std::collections::BTreeSet::new();
        for (_, v) in self.meta.scan(KeyRange::prefix(prefix)).await? {
            orgs.insert(decode_record::<AuditRecord>(&v)?.org);
        }
        Ok(orgs.into_iter().collect())
    }

    /// Up to `limit` of `org`'s queued records, oldest first, with their
    /// keys.
    pub(super) async fn pending(
        &self,
        org: &str,
        limit: usize,
    ) -> Result<Vec<(bytes::Bytes, AuditRecord)>> {
        let prefix = keys::identity_row(table::AUDIT_PENDING, org);
        self.meta
            .scan(KeyRange::prefix(prefix.clone()).limit(limit))
            .await?
            .into_iter()
            .map(|(k, v)| Ok((k, decode_record(&v)?)))
            .collect()
    }
}

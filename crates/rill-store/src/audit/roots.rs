//! Signed seals over each org's chain.

use rill_types::audit::AuditRoot;

use super::store::AuditStore;
use crate::error::Result;
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

impl AuditStore {
    /// Store a root; refused when one already seals that entry.
    pub async fn put_root(&self, root: &AuditRoot) -> Result<()> {
        let key = keys::identity_pair(table::AUDIT_ROOT, root.org.as_str(), root.last_seq as i64);
        let tx = Transaction::new()
            .expect_absent(key.clone())
            .set(key, encode_record(root)?);
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// `org`'s roots, oldest first.
    pub async fn roots(&self, org: &str) -> Result<Vec<AuditRoot>> {
        let prefix = keys::identity_row(table::AUDIT_ROOT, org);
        self.meta
            .scan(KeyRange::prefix(prefix))
            .await?
            .iter()
            .map(|(_, v)| decode_record(v))
            .collect()
    }

    /// The newest root of `org`.
    pub async fn latest_root(&self, org: &str) -> Result<Option<AuditRoot>> {
        let prefix = keys::identity_row(table::AUDIT_ROOT, org);
        let rows = self
            .meta
            .scan(KeyRange::prefix(prefix).reverse().limit(1))
            .await?;
        rows.first().map(|(_, v)| decode_record(v)).transpose()
    }
}

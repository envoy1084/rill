//! Versions held back because secret scanning found something in them and
//! the repository quarantines rather than blocks.

use bytes::Bytes;
use rill_types::ChangeVersionHash;

use super::store::RepoStore;
use crate::error::Result;
use crate::meta::{Mutation, Transaction};

impl RepoStore {
    /// The mutation quarantining `version`, with why, for its commit.
    pub fn quarantine_mutation(&self, version: &ChangeVersionHash, why: &str) -> Mutation {
        Mutation::Set {
            key: self.keys().quarantine(version),
            value: Bytes::copy_from_slice(why.as_bytes()),
        }
    }

    /// Why `version` is quarantined, if it is.
    pub async fn quarantined(&self, version: &ChangeVersionHash) -> Result<Option<String>> {
        Ok(self
            .meta()
            .get(&self.keys().quarantine(version))
            .await?
            .map(|b| String::from_utf8_lossy(&b).into_owned()))
    }

    /// Release `version`, with `extra` mutations (its audit record) in the
    /// same transaction.
    pub async fn clear_quarantine(
        &self,
        version: &ChangeVersionHash,
        extra: Vec<Mutation>,
    ) -> Result<()> {
        let key = self.keys().quarantine(version);
        let mut tx = Transaction::new().expect_present(key.clone()).delete(key);
        tx.mutations.extend(extra);
        self.meta().transact(tx).await?;
        Ok(())
    }
}

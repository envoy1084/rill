//! Versions held back because secret scanning found something in them and
//! the repository quarantines rather than blocks.

use bytes::Bytes;
use rill_types::ChangeVersionHash;

use super::store::RepoStore;
use crate::error::Result;
use crate::meta::{Mutation, Transaction};

/// Ancestors off trunk walked before giving up.
const MAX_ANCESTRY: usize = 10_000;

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

    /// Why `version` may not reach trunk: it, or an ancestor not yet on
    /// trunk, is quarantined. Moving trunk to a version brings in all of
    /// its history, so every such ancestor counts.
    pub async fn quarantined_ancestry(
        &self,
        version: &ChangeVersionHash,
    ) -> Result<Option<String>> {
        let mut queue = std::collections::VecDeque::from([*version]);
        let mut seen = std::collections::HashSet::new();
        while let Some(at) = queue.pop_front() {
            if !seen.insert(at) || self.trunk_seq_of(&at).await?.is_some() {
                continue;
            }
            if seen.len() > MAX_ANCESTRY {
                return Ok(Some(format!(
                    "more than {MAX_ANCESTRY} of its ancestors are not on trunk; land them first"
                )));
            }
            if let Some(why) = self.quarantined(&at).await? {
                return Ok(Some(if at == *version {
                    why
                } else {
                    format!("its ancestor {at} is quarantined: {why}")
                }));
            }
            if let Some(cv) = self.get_change_version(&at).await? {
                queue.extend(cv.parents);
            }
        }
        Ok(None)
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

//! Relationship tuples for ReBAC.

use bytes::Bytes;

use super::store::RepoStore;
use crate::error::Result;
use crate::keys::{table, tail};
use crate::meta::{KeyRange, Transaction};

impl RepoStore {
    pub async fn add_relation(&self, object: &str, relation: &str, subject: &str) -> Result<()> {
        self.meta()
            .set(
                self.keys().relation(object, relation, subject),
                Bytes::new(),
            )
            .await
    }

    pub async fn remove_relation(&self, object: &str, relation: &str, subject: &str) -> Result<()> {
        self.meta()
            .transact(Transaction::new().delete(self.keys().relation(object, relation, subject)))
            .await?;
        Ok(())
    }

    /// All `(relation, subject)` tuples on an object.
    pub async fn relations_of(&self, object: &str) -> Result<Vec<(String, String)>> {
        let prefix = self.keys().relations_of(object);
        let rows = self.meta().scan(KeyRange::prefix(prefix.clone())).await?;
        rows.iter().map(|(k, _)| tail(&prefix, k)).collect()
    }

    /// Every `(object, relation, subject)` tuple in the repository.
    pub async fn list_relations(&self) -> Result<Vec<(String, String, String)>> {
        let prefix = self.keys().table(table::REBAC);
        let rows = self.meta().scan(KeyRange::prefix(prefix.clone())).await?;
        rows.iter().map(|(k, _)| tail(&prefix, k)).collect()
    }
}

//! Relationship tuples for ReBAC.

use bytes::Bytes;
use rill_types::{EventKind, Principal};

use super::store::RepoStore;
use crate::error::Result;
use crate::keys::{table, tail};
use crate::meta::{KeyRange, Transaction};

impl RepoStore {
    pub async fn add_relation(
        &self,
        object: &str,
        relation: &str,
        subject: &str,
        actor: &Principal,
    ) -> Result<()> {
        let tx = Transaction::new().set(
            self.keys().relation(object, relation, subject),
            Bytes::new(),
        );
        self.write_relation(tx, object, relation, subject, true, actor)
            .await
    }

    pub async fn remove_relation(
        &self,
        object: &str,
        relation: &str,
        subject: &str,
        actor: &Principal,
    ) -> Result<()> {
        let tx = Transaction::new().delete(self.keys().relation(object, relation, subject));
        self.write_relation(tx, object, relation, subject, false, actor)
            .await
    }

    async fn write_relation(
        &self,
        mut tx: Transaction,
        object: &str,
        relation: &str,
        subject: &str,
        added: bool,
        actor: &Principal,
    ) -> Result<()> {
        tx.mutations.extend(self.event_by(
            actor,
            EventKind::RelationChanged {
                object: object.to_string(),
                relation: relation.to_string(),
                subject: subject.to_string(),
                added,
            },
        )?);
        self.meta().transact(tx).await?;
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

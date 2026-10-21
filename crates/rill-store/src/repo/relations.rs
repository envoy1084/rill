//! Relationship tuples for ReBAC, kept by object and by subject, with a
//! version counter every change bumps in its own transaction.

use bytes::Bytes;
use rill_types::{EventKind, Principal};

use super::store::RepoStore;
use crate::error::Result;
use crate::keys::{table, tail};
use crate::meta::{KeyRange, Mutation, Transaction};

impl RepoStore {
    /// Add a tuple; returns the relation version after the change.
    pub async fn add_relation(
        &self,
        object: &str,
        relation: &str,
        subject: &str,
        actor: &Principal,
    ) -> Result<i64> {
        let tx = Transaction::new()
            .set(
                self.keys().relation(object, relation, subject),
                Bytes::new(),
            )
            .set(
                self.keys().relation_by_subject(subject, object, relation),
                Bytes::new(),
            );
        self.write_relation(tx, object, relation, subject, true, actor)
            .await
    }

    /// Remove a tuple; returns the relation version after the change.
    pub async fn remove_relation(
        &self,
        object: &str,
        relation: &str,
        subject: &str,
        actor: &Principal,
    ) -> Result<i64> {
        let tx = Transaction::new()
            .delete(self.keys().relation(object, relation, subject))
            .delete(self.keys().relation_by_subject(subject, object, relation));
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
    ) -> Result<i64> {
        tx.mutations.push(Mutation::Add {
            key: self.keys().table(table::REBAC_VERSION),
            delta: 1,
        });
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
        self.relations_version().await
    }

    /// The relation version: how many changes have been made. A snapshot
    /// read after observing version `v` reflects at least those changes.
    pub async fn relations_version(&self) -> Result<i64> {
        self.meta()
            .counter(&self.keys().table(table::REBAC_VERSION))
            .await
    }

    /// All `(relation, subject)` tuples on an object.
    pub async fn relations_of(&self, object: &str) -> Result<Vec<(String, String)>> {
        let prefix = self.keys().relations_of(object);
        let rows = self.meta().scan(KeyRange::prefix(prefix.clone())).await?;
        rows.iter().map(|(k, _)| tail(&prefix, k)).collect()
    }

    /// All `(object, relation)` tuples granted to a subject.
    pub async fn relations_of_subject(&self, subject: &str) -> Result<Vec<(String, String)>> {
        let prefix = self.keys().relations_of_subject(subject);
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

//! Relationship tuples for ReBAC, kept by object and by subject, with a
//! version every change advances in its own transaction. The version is a
//! plain value rather than an atomic counter so a commit can assert it:
//! a relation change racing an authorized write then conflicts with it.

use bytes::Bytes;
use rill_types::{EventKind, Principal};

use super::store::RepoStore;
use crate::error::Result;
use crate::keys::{table, tail};
use crate::meta::{KeyRange, Transaction};

/// Attempts at a relation change when another one races it.
const RELATION_ATTEMPTS: usize = 16;

fn encode_version(v: i64) -> Bytes {
    Bytes::copy_from_slice(&v.to_be_bytes())
}

fn decode_version(raw: Option<&[u8]>) -> Result<i64> {
    match raw {
        None => Ok(0),
        Some(b) => <[u8; 8]>::try_from(b)
            .map(i64::from_be_bytes)
            .map_err(|_| crate::Error::Codec("malformed relation version".into())),
    }
}

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
        tx: Transaction,
        object: &str,
        relation: &str,
        subject: &str,
        added: bool,
        actor: &Principal,
    ) -> Result<i64> {
        let key = self.keys().table(table::REBAC_VERSION);
        for _ in 0..RELATION_ATTEMPTS {
            let current = self.relations_version().await?;
            let next = current + 1;
            let mut attempt = self
                .expect_relations_version(tx.clone(), current)
                .set(key.clone(), encode_version(next));
            attempt.mutations.extend(self.event_by(
                actor,
                EventKind::RelationChanged {
                    object: object.to_string(),
                    relation: relation.to_string(),
                    subject: subject.to_string(),
                    added,
                },
            )?);
            match self.meta().transact(attempt).await {
                Ok(_) => return Ok(next),
                Err(e) if e.is_conflict() => continue,
                Err(e) => return Err(e),
            }
        }
        Err(crate::Error::Conflict(
            "relations kept changing; retry".into(),
        ))
    }

    /// The relation version: how many changes have been made. A snapshot
    /// read after observing version `v` reflects at least those changes.
    pub async fn relations_version(&self) -> Result<i64> {
        let raw = self
            .meta()
            .get(&self.keys().table(table::REBAC_VERSION))
            .await?;
        decode_version(raw.as_deref())
    }

    /// `tx` holding only while the relation version is still `version`.
    pub fn expect_relations_version(&self, tx: Transaction, version: i64) -> Transaction {
        let key = self.keys().table(table::REBAC_VERSION);
        tx.expect(key, (version != 0).then(|| encode_version(version)))
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

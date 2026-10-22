//! Refs and the trunk sequence.

use bytes::Bytes;
use rill_types::{ChangeVersionHash, EventKind, Line, Principal, RefName, TrunkSeq};

use super::codec::{hash_value, parse_hash, parse_seq};
use super::store::RepoStore;
use crate::error::{Error, Result};
use crate::keys::{table, tail};
use crate::meta::{seq_from_key, KeyRange, Transaction};

/// The trunk-head row: be64 sequence then the version hash.
pub(super) fn head_row(seq: u64, version: &ChangeVersionHash) -> Bytes {
    let mut row = seq.to_be_bytes().to_vec();
    row.extend_from_slice(version.as_bytes());
    Bytes::from(row)
}

pub(super) fn parse_head(row: &[u8]) -> Result<(TrunkSeq, ChangeVersionHash)> {
    if row.len() != 40 {
        return Err(Error::Codec("bad trunk head".into()));
    }
    Ok((TrunkSeq(parse_seq(&row[..8])?), parse_hash(&row[8..])?))
}

impl RepoStore {
    pub async fn get_ref(&self, name: &RefName) -> Result<Option<ChangeVersionHash>> {
        match self.meta().get(&self.keys().reference(name)).await? {
            Some(bytes) => Ok(Some(parse_hash(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Compare-and-swap a ref. `expected == None` requires the ref to be
    /// absent; `target == None` deletes it.
    pub async fn update_ref(
        &self,
        name: &RefName,
        expected: Option<ChangeVersionHash>,
        target: Option<ChangeVersionHash>,
        actor: &Principal,
    ) -> Result<()> {
        let key = self.keys().reference(name);
        let mut tx = Transaction::new().expect(key.clone(), expected.as_ref().map(hash_value));
        tx = match target {
            Some(h) => tx.set(key, hash_value(&h)),
            None => tx.delete(key),
        };
        tx.mutations.extend(self.event_by(
            actor,
            EventKind::RefMoved {
                name: name.to_string(),
                old: expected,
                new: target,
            },
        )?);
        self.meta().transact(tx).await?;
        Ok(())
    }

    pub async fn list_refs(&self) -> Result<Vec<(RefName, ChangeVersionHash)>> {
        let prefix = self.keys().table(table::REF);
        let rows = self.meta().scan(KeyRange::prefix(prefix.clone())).await?;
        rows.iter()
            .map(|(k, v)| {
                let name: String = tail(&prefix, k)?;
                Ok((RefName::new(&name)?, parse_hash(v)?))
            })
            .collect()
    }

    /// The newest trunk position and the version there.
    pub async fn trunk_head(&self) -> Result<Option<(TrunkSeq, ChangeVersionHash)>> {
        self.line_head(&Line::Trunk).await
    }

    pub async fn trunk_at(&self, seq: TrunkSeq) -> Result<Option<ChangeVersionHash>> {
        self.line_at(&Line::Trunk, seq).await
    }

    pub async fn trunk_seq_of(&self, hash: &ChangeVersionHash) -> Result<Option<TrunkSeq>> {
        self.line_seq_of(&Line::Trunk, hash).await
    }

    /// The newest position of `line` and the version there. A release
    /// starts at position 0, the version it was cut at.
    pub async fn line_head(&self, line: &Line) -> Result<Option<(TrunkSeq, ChangeVersionHash)>> {
        match self.meta().get(&self.keys().line(line).head()).await? {
            Some(row) => Ok(Some(parse_head(&row)?)),
            None => Ok(None),
        }
    }

    /// The version at position `seq` of `line`.
    pub async fn line_at(&self, line: &Line, seq: TrunkSeq) -> Result<Option<ChangeVersionHash>> {
        match self
            .meta()
            .get(&self.keys().line(line).position(seq.get()))
            .await?
        {
            Some(bytes) => Ok(Some(parse_hash(&bytes)?)),
            None => Ok(None),
        }
    }

    /// The position `hash` landed at on `line`, if it did.
    pub async fn line_seq_of(
        &self,
        line: &Line,
        hash: &ChangeVersionHash,
    ) -> Result<Option<TrunkSeq>> {
        match self
            .meta()
            .get(&self.keys().line(line).by_hash(hash))
            .await?
        {
            Some(bytes) => Ok(Some(TrunkSeq(parse_seq(&bytes)?))),
            None => Ok(None),
        }
    }

    /// Cut `release/<name>` at `at`: the ref and the release's head row
    /// (position 0), written together. Fails with a conflict when the
    /// release exists.
    pub async fn cut_release(
        &self,
        name: &str,
        at: ChangeVersionHash,
        actor: &Principal,
    ) -> Result<()> {
        let ref_name = RefName::release(name)?;
        let line = Line::Release(name.to_string());
        let keys = self.keys();
        let head = keys.line(&line).head();
        let reference = keys.reference(&ref_name);
        let mut tx = Transaction::new()
            .expect_absent(reference.clone())
            .expect_absent(head.clone())
            .set(reference, hash_value(&at))
            .set(head, head_row(0, &at));
        tx.mutations.extend(self.event_by(
            actor,
            EventKind::RefMoved {
                name: ref_name.to_string(),
                old: None,
                new: Some(at),
            },
        )?);
        self.meta().transact(tx).await?;
        Ok(())
    }

    /// Trunk entries in `(from, to]`, oldest first.
    pub async fn trunk_range(
        &self,
        from: TrunkSeq,
        to: TrunkSeq,
    ) -> Result<Vec<(TrunkSeq, ChangeVersionHash)>> {
        let prefix = self.keys().trunk_table();
        let rows = self
            .meta()
            .scan(KeyRange::between(
                self.keys().trunk(from.get() + 1),
                self.keys().trunk(to.get() + 1),
            ))
            .await?;
        rows.iter()
            .map(|(k, v)| {
                let seq = seq_from_key(prefix.len(), k)
                    .ok_or_else(|| Error::Codec("bad trunk key".into()))?;
                Ok((TrunkSeq(seq), parse_hash(v)?))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rill_types::{ChangeId, ChangeVersion, Principal, Provenance, Timestamp};

    use crate::Stores;

    use super::*;

    fn cv(msg: &str, parents: Vec<ChangeVersionHash>) -> ChangeVersion {
        ChangeVersion {
            change_id: ChangeId::new(),
            parents,
            predecessor: None,
            subtree_changes: BTreeMap::new(),
            file_changes: BTreeMap::new(),
            author: Principal::human("a"),
            provenance: Provenance::default(),
            intent: None,
            message: msg.into(),
            created_at: Timestamp::from_millis(1),
            extra: BTreeMap::new(),
        }
    }

    #[tokio::test]
    async fn land_assigns_sequences_and_casts() {
        let stores = Stores::memory();
        let config = stores.create_repo("r", "").await.unwrap();
        let repo = stores.repo(config.id);
        let a = repo.put_change_version(&cv("a", vec![])).await.unwrap();
        let b = repo.put_change_version(&cv("b", vec![a])).await.unwrap();
        assert_eq!(repo.land_on_trunk(None, a).await.unwrap(), TrunkSeq(1));
        assert!(repo.land_on_trunk(None, b).await.unwrap_err().is_conflict());
        assert_eq!(repo.land_on_trunk(Some(a), b).await.unwrap(), TrunkSeq(2));
        assert_eq!(repo.trunk_seq_of(&b).await.unwrap(), Some(TrunkSeq(2)));
        assert_eq!(repo.trunk_head().await.unwrap(), Some((TrunkSeq(2), b)));
        let log = repo.log_after(None, 10).await.unwrap();
        assert_eq!(log.iter().map(|(_, h)| *h).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(repo.log_head().await.unwrap(), Some(log[1].0));
        assert_eq!(
            repo.log_after(Some(log[0].0), 10).await.unwrap(),
            vec![log[1]]
        );
        let info = repo.get_change_info(&b).await.unwrap().unwrap();
        assert_eq!(info.trunk_seq, Some(TrunkSeq(2)));
        assert_eq!(
            repo.trunk_range(TrunkSeq(0), TrunkSeq(2))
                .await
                .unwrap()
                .len(),
            2
        );
    }
}

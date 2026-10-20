//! Refs and the trunk sequence.

use bytes::Bytes;
use rill_types::{ChangeVersionHash, RefName, TrunkSeq};

use super::codec::{hash_value, parse_hash, parse_seq};
use super::store::RepoStore;
use crate::error::{Error, Result};
use crate::keys::{table, tail};
use crate::meta::{seq_from_key, KeyRange, Transaction};

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
    ) -> Result<()> {
        let key = self.keys().reference(name);
        let mut tx = Transaction::new().expect(key.clone(), expected.as_ref().map(hash_value));
        tx = match target {
            Some(h) => tx.set(key, hash_value(&h)),
            None => tx.delete(key),
        };
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

    /// Advance trunk to `hash` if it currently points at `expected`, assigning
    /// the next sequence number. The whole update is one transaction, so two
    /// landers racing for the same slot see exactly one winner.
    pub async fn land_on_trunk(
        &self,
        expected: Option<ChangeVersionHash>,
        hash: ChangeVersionHash,
    ) -> Result<TrunkSeq> {
        let (head_seq, head_hash) = match self.trunk_head().await? {
            Some((s, h)) => (s.get(), Some(h)),
            None => (0, None),
        };
        if head_hash != expected {
            return Err(Error::Conflict("trunk moved".into()));
        }
        let seq = head_seq + 1;
        let keys = self.keys();
        let ref_key = keys.reference(&RefName::trunk());
        let tx = Transaction::new()
            .expect(ref_key.clone(), expected.as_ref().map(hash_value))
            .expect_absent(keys.trunk(seq))
            .set(ref_key, hash_value(&hash))
            .set(keys.trunk(seq), hash_value(&hash))
            .set(
                keys.trunk_by_hash(&hash),
                Bytes::copy_from_slice(&seq.to_be_bytes()),
            );
        self.meta().transact(tx).await?;
        Ok(TrunkSeq(seq))
    }

    pub async fn trunk_head(&self) -> Result<Option<(TrunkSeq, ChangeVersionHash)>> {
        let prefix = self.keys().trunk_table();
        let rows = self
            .meta()
            .scan(KeyRange::prefix(prefix.clone()).reverse().limit(1))
            .await?;
        match rows.first() {
            Some((k, v)) => {
                let seq = seq_from_key(prefix.len(), k)
                    .ok_or_else(|| Error::Codec("bad trunk key".into()))?;
                Ok(Some((TrunkSeq(seq), parse_hash(v)?)))
            }
            None => Ok(None),
        }
    }

    pub async fn trunk_at(&self, seq: TrunkSeq) -> Result<Option<ChangeVersionHash>> {
        match self.meta().get(&self.keys().trunk(seq.get())).await? {
            Some(bytes) => Ok(Some(parse_hash(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn trunk_seq_of(&self, hash: &ChangeVersionHash) -> Result<Option<TrunkSeq>> {
        match self.meta().get(&self.keys().trunk_by_hash(hash)).await? {
            Some(bytes) => Ok(Some(TrunkSeq(parse_seq(&bytes)?))),
            None => Ok(None),
        }
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

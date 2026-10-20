//! Typed access to one repository's data.

use std::sync::Arc;

use bytes::Bytes;
use rill_types::{
    ChangeId, ChangeInfo, ChangeVersion, ChangeVersionHash, Encodable, Intent, IntentId,
    ObjectHash, ObjectKind, Operation, OperationId, Record, RefName, RepoId, ReviewState,
    Signature, Timestamp, TrunkSeq, Workspace, WorkspaceId,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::keys;
use crate::meta::{seq_from_key, seq_key, KeyRange, MetaStore, Transaction};
use crate::object::{ObjectStore, ObjectStoreExt};

/// Encode a mutable record (not content-addressed) with its type tag.
pub fn encode_record<T: Record>(value: &T) -> Result<Bytes> {
    Ok(Bytes::from(rill_types::encode_record(value)))
}

/// Decode a record, refusing other record types and schemas.
pub fn decode_record<T: Record>(bytes: &[u8]) -> Result<T> {
    rill_types::decode_record(bytes).map_err(|e| Error::Codec(e.to_string()))
}

/// Encode a value that is not a [`Record`] under an explicit tag, usually
/// the storage kind it is kept under.
pub fn encode_tagged<T: Serialize + ?Sized>(tag: &str, value: &T) -> Result<Bytes> {
    Ok(Bytes::from(rill_types::encode_tagged(tag, 1, value)))
}

/// Decode a value written by [`encode_tagged`] with the same tag.
pub fn decode_tagged<T: DeserializeOwned>(tag: &str, bytes: &[u8]) -> Result<T> {
    rill_types::decode_tagged(tag, 1, bytes).map_err(|e| Error::Codec(e.to_string()))
}

/// Per-repository configuration record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoConfig {
    pub id: RepoId,
    pub name: String,
    pub description: String,
    pub created_at: Timestamp,
    /// Trunk ref name; always `trunk` today but stored so it can change.
    pub trunk: RefName,
}

impl Record for RepoConfig {
    const TAG: &'static str = "repo";
}

/// The pair of backends a deployment is built on.
#[derive(Clone)]
pub struct Stores {
    pub objects: Arc<dyn ObjectStore>,
    pub meta: Arc<dyn MetaStore>,
}

/// Format of everything a store holds. Changing any encoding bumps it, and
/// a build opens only stores of its own format.
pub const STORE_FORMAT: u32 = 1;

impl Stores {
    pub fn new(objects: Arc<dyn ObjectStore>, meta: Arc<dyn MetaStore>) -> Self {
        Stores { objects, meta }
    }

    pub fn memory() -> Self {
        Stores {
            objects: Arc::new(crate::memory::MemoryObjectStore::new()),
            meta: Arc::new(crate::memory::MemoryMetaStore::new()),
        }
    }

    /// Stamp an empty store with [`STORE_FORMAT`], or refuse a store
    /// written in another format.
    pub async fn check_format(&self) -> Result<()> {
        let key = keys::store_format();
        match self.meta.get(&key).await? {
            Some(bytes) => {
                let found = <[u8; 4]>::try_from(bytes.as_ref())
                    .map(u32::from_be_bytes)
                    .map_err(|_| Error::Codec("malformed store format marker".into()))?;
                if found != STORE_FORMAT {
                    return Err(Error::IncompatibleFormat {
                        found,
                        expected: STORE_FORMAT,
                    });
                }
                Ok(())
            }
            None => {
                let tx = Transaction::new()
                    .expect_absent(key.clone())
                    .set(key, Bytes::copy_from_slice(&STORE_FORMAT.to_be_bytes()));
                match self.meta.transact(tx).await {
                    Ok(_) | Err(Error::Conflict(_)) => Ok(()),
                    Err(e) => Err(e),
                }
            }
        }
    }

    pub fn repo(&self, id: RepoId) -> RepoStore {
        RepoStore {
            stores: self.clone(),
            id,
        }
    }

    pub async fn create_repo(&self, name: &str, description: &str) -> Result<RepoConfig> {
        let config = RepoConfig {
            id: RepoId::new(),
            name: name.to_string(),
            description: description.to_string(),
            created_at: Timestamp::now(),
            trunk: RefName::trunk(),
        };
        let tx = Transaction::new()
            .expect_absent(keys::repo_by_name(name))
            .set(keys::repo(&config.id), encode_record(&config)?)
            .set(
                keys::repo_by_name(name),
                Bytes::copy_from_slice(config.id.as_bytes()),
            );
        self.meta.transact(tx).await.map_err(|e| match e {
            Error::Conflict(_) => Error::Conflict(format!("repo {name:?} already exists")),
            other => other,
        })?;
        Ok(config)
    }

    pub async fn get_repo(&self, id: &RepoId) -> Result<Option<RepoConfig>> {
        match self.meta.get(&keys::repo(id)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn get_repo_by_name(&self, name: &str) -> Result<Option<RepoConfig>> {
        match self.meta.get(&keys::repo_by_name(name)).await? {
            Some(bytes) => {
                let arr: [u8; 16] = bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| Error::Codec("bad repo id".into()))?;
                self.get_repo(&RepoId::from_bytes(arr)).await
            }
            None => Ok(None),
        }
    }

    pub async fn list_repos(&self) -> Result<Vec<RepoConfig>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::repo_list_prefix()))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }
}

/// Typed operations on one repository.
#[derive(Clone)]
pub struct RepoStore {
    stores: Stores,
    id: RepoId,
}

impl RepoStore {
    pub fn id(&self) -> RepoId {
        self.id
    }

    pub fn objects(&self) -> &Arc<dyn ObjectStore> {
        &self.stores.objects
    }

    pub fn meta(&self) -> &Arc<dyn MetaStore> {
        &self.stores.meta
    }

    pub fn stores(&self) -> &Stores {
        &self.stores
    }

    // ---- change versions -------------------------------------------------

    /// Store a change version: the object itself, its info projection, an
    /// entry in the write-ahead log and in the change's version list.
    /// Idempotent: storing the same version twice is a no-op.
    pub async fn put_change_version(&self, cv: &ChangeVersion) -> Result<ChangeVersionHash> {
        let hash = cv.hash();
        if self
            .meta()
            .get(&keys::change_info(&self.id, &hash))
            .await?
            .is_some()
        {
            return Ok(hash);
        }
        self.objects().put_object(cv).await?;
        let info = ChangeInfo::from_version(cv, None);
        let tx = Transaction::new()
            .set(keys::change_info(&self.id, &hash), encode_record(&info)?)
            .append_log(
                keys::log_prefix(&self.id),
                Bytes::copy_from_slice(hash.as_bytes()),
            )
            .append_log(
                keys::change_versions_prefix(&self.id, &cv.change_id),
                Bytes::copy_from_slice(hash.as_bytes()),
            );
        self.meta().transact(tx).await?;
        Ok(hash)
    }

    pub async fn get_change_version(
        &self,
        hash: &ChangeVersionHash,
    ) -> Result<Option<ChangeVersion>> {
        self.objects().get_object(hash).await
    }

    pub async fn get_change_version_required(
        &self,
        hash: &ChangeVersionHash,
    ) -> Result<ChangeVersion> {
        self.objects().get_object_required(hash).await
    }

    pub async fn get_change_info(&self, hash: &ChangeVersionHash) -> Result<Option<ChangeInfo>> {
        match self.meta().get(&keys::change_info(&self.id, hash)).await? {
            Some(bytes) => {
                let mut info: ChangeInfo = decode_record(&bytes)?;
                info.trunk_seq = self.trunk_seq_of(hash).await?;
                Ok(Some(info))
            }
            None => Ok(None),
        }
    }

    pub async fn has_change_version(&self, hash: &ChangeVersionHash) -> Result<bool> {
        Ok(self
            .meta()
            .get(&keys::change_info(&self.id, hash))
            .await?
            .is_some())
    }

    /// Versions of a change in creation order.
    pub async fn change_versions(&self, id: &ChangeId) -> Result<Vec<ChangeVersionHash>> {
        let rows = self
            .meta()
            .scan(KeyRange::prefix(keys::change_versions_prefix(&self.id, id)))
            .await?;
        rows.iter()
            .map(|(_, v)| ObjectHash::from_slice(v).map_err(Error::from))
            .collect()
    }

    /// Entries of the write-ahead log after `after` (exclusive), oldest first.
    pub async fn log_after(
        &self,
        after: u64,
        limit: usize,
    ) -> Result<Vec<(u64, ChangeVersionHash)>> {
        let prefix = keys::log_prefix(&self.id);
        let start = seq_key(&prefix, after + 1);
        let end = crate::meta::prefix_end(&prefix).expect("log prefix is not all 0xFF");
        let rows = self
            .meta()
            .scan(KeyRange::between(start, end).limit(limit))
            .await?;
        rows.iter()
            .map(|(k, v)| {
                let seq = seq_from_key(prefix.len(), k)
                    .ok_or_else(|| Error::Codec("bad log key".into()))?;
                Ok((seq, ObjectHash::from_slice(v)?))
            })
            .collect()
    }

    pub async fn log_head(&self) -> Result<u64> {
        let rows = self
            .meta()
            .scan(
                KeyRange::prefix(keys::log_prefix(&self.id))
                    .reverse()
                    .limit(1),
            )
            .await?;
        Ok(rows
            .first()
            .and_then(|(k, _)| seq_from_key(keys::log_prefix(&self.id).len(), k))
            .unwrap_or(0))
    }

    pub async fn put_signatures(
        &self,
        hash: &ChangeVersionHash,
        signatures: &[Signature],
    ) -> Result<()> {
        self.meta()
            .set(
                keys::signatures(&self.id, hash),
                encode_tagged("signatures", signatures)?,
            )
            .await
    }

    pub async fn get_signatures(&self, hash: &ChangeVersionHash) -> Result<Vec<Signature>> {
        match self.meta().get(&keys::signatures(&self.id, hash)).await? {
            Some(bytes) => decode_tagged("signatures", &bytes),
            None => Ok(Vec::new()),
        }
    }

    // ---- refs and trunk --------------------------------------------------

    pub async fn get_ref(&self, name: &RefName) -> Result<Option<ChangeVersionHash>> {
        match self.meta().get(&keys::reference(&self.id, name)).await? {
            Some(bytes) => Ok(Some(ObjectHash::from_slice(&bytes)?)),
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
        let key = keys::reference(&self.id, name);
        let mut tx = Transaction::new().expect(
            key.clone(),
            expected.map(|h| Bytes::copy_from_slice(h.as_bytes())),
        );
        tx = match target {
            Some(h) => tx.set(key, Bytes::copy_from_slice(h.as_bytes())),
            None => tx.delete(key),
        };
        self.meta().transact(tx).await?;
        Ok(())
    }

    pub async fn list_refs(&self) -> Result<Vec<(RefName, ChangeVersionHash)>> {
        let prefix = keys::ref_prefix(&self.id);
        let rows = self.meta().scan(KeyRange::prefix(prefix.clone())).await?;
        rows.iter()
            .map(|(k, v)| {
                let name = std::str::from_utf8(&k[prefix.len()..])
                    .map_err(|e| Error::Codec(e.to_string()))?;
                Ok((RefName::new(name)?, ObjectHash::from_slice(v)?))
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
        let ref_key = keys::reference(&self.id, &RefName::trunk());
        let tx = Transaction::new()
            .expect(
                ref_key.clone(),
                expected.map(|h| Bytes::copy_from_slice(h.as_bytes())),
            )
            .expect_absent(keys::trunk(&self.id, seq))
            .set(ref_key, Bytes::copy_from_slice(hash.as_bytes()))
            .set(
                keys::trunk(&self.id, seq),
                Bytes::copy_from_slice(hash.as_bytes()),
            )
            .set(
                keys::trunk_by_hash(&self.id, &hash),
                Bytes::copy_from_slice(&seq.to_be_bytes()),
            );
        self.meta().transact(tx).await?;
        Ok(TrunkSeq(seq))
    }

    pub async fn trunk_head(&self) -> Result<Option<(TrunkSeq, ChangeVersionHash)>> {
        let prefix = keys::trunk_prefix(&self.id);
        let rows = self
            .meta()
            .scan(KeyRange::prefix(prefix.clone()).reverse().limit(1))
            .await?;
        match rows.first() {
            Some((k, v)) => {
                let seq = seq_from_key(prefix.len(), k)
                    .ok_or_else(|| Error::Codec("bad trunk key".into()))?;
                Ok(Some((TrunkSeq(seq), ObjectHash::from_slice(v)?)))
            }
            None => Ok(None),
        }
    }

    pub async fn trunk_at(&self, seq: TrunkSeq) -> Result<Option<ChangeVersionHash>> {
        match self.meta().get(&keys::trunk(&self.id, seq.get())).await? {
            Some(bytes) => Ok(Some(ObjectHash::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn trunk_seq_of(&self, hash: &ChangeVersionHash) -> Result<Option<TrunkSeq>> {
        match self
            .meta()
            .get(&keys::trunk_by_hash(&self.id, hash))
            .await?
        {
            Some(bytes) => {
                let arr: [u8; 8] = bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| Error::Codec("bad trunk seq".into()))?;
                Ok(Some(TrunkSeq(u64::from_be_bytes(arr))))
            }
            None => Ok(None),
        }
    }

    /// Trunk entries in `(from, to]`, oldest first.
    pub async fn trunk_range(
        &self,
        from: TrunkSeq,
        to: TrunkSeq,
    ) -> Result<Vec<(TrunkSeq, ChangeVersionHash)>> {
        let prefix = keys::trunk_prefix(&self.id);
        let rows = self
            .meta()
            .scan(KeyRange::between(
                seq_key(&prefix, from.get() + 1),
                seq_key(&prefix, to.get() + 1),
            ))
            .await?;
        rows.iter()
            .map(|(k, v)| {
                let seq = seq_from_key(prefix.len(), k)
                    .ok_or_else(|| Error::Codec("bad trunk key".into()))?;
                Ok((TrunkSeq(seq), ObjectHash::from_slice(v)?))
            })
            .collect()
    }

    // ---- review state ----------------------------------------------------

    pub async fn get_review(&self, id: &ChangeId) -> Result<Option<ReviewState>> {
        match self.meta().get(&keys::change(&self.id, id)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Write a review state. When `expected` is given, the write succeeds
    /// only if the stored record still equals it (optimistic concurrency).
    pub async fn put_review(
        &self,
        state: &ReviewState,
        expected: Option<&ReviewState>,
    ) -> Result<()> {
        let key = keys::change(&self.id, &state.change_id);
        let mut tx = Transaction::new();
        if let Some(prev) = expected {
            tx = tx.expect(key.clone(), Some(encode_record(prev)?));
        }
        tx = tx.set(key, encode_record(state)?);
        self.meta().transact(tx).await?;
        Ok(())
    }

    pub async fn list_reviews(&self) -> Result<Vec<ReviewState>> {
        let rows = self
            .meta()
            .scan(KeyRange::prefix(keys::repo_prefix(
                keys::table::CHANGE,
                &self.id,
            )))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    // ---- workspaces and operations ----------------------------------------

    pub async fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>> {
        match self.meta().get(&keys::workspace(&self.id, id)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn put_workspace(&self, ws: &Workspace) -> Result<()> {
        self.meta()
            .set(keys::workspace(&self.id, &ws.id), encode_record(ws)?)
            .await
    }

    pub async fn delete_workspace(&self, id: &WorkspaceId) -> Result<()> {
        self.meta()
            .transact(Transaction::new().delete(keys::workspace(&self.id, id)))
            .await?;
        Ok(())
    }

    pub async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        let rows = self
            .meta()
            .scan(KeyRange::prefix(keys::workspace_prefix(&self.id)))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    /// Append an operation and update the workspace in one transaction.
    /// Returns the operation's sequence within the workspace log.
    pub async fn append_operation(&self, ws: &Workspace, op: &Operation) -> Result<u64> {
        let tx = Transaction::new()
            .expect_absent(keys::operation_index(&self.id, &op.id))
            .append_log(keys::operation_prefix(&self.id, &ws.id), encode_record(op)?)
            .set(keys::workspace(&self.id, &ws.id), encode_record(ws)?);
        let out = self.meta().transact(tx).await?;
        let seq = out.appended.first().copied().unwrap_or(0);
        self.meta()
            .set(
                keys::operation_index(&self.id, &op.id),
                Bytes::copy_from_slice(&seq.to_be_bytes()),
            )
            .await?;
        Ok(seq)
    }

    pub async fn has_operation(&self, op: &OperationId) -> Result<bool> {
        Ok(self
            .meta()
            .get(&keys::operation_index(&self.id, op))
            .await?
            .is_some())
    }

    /// Operations of a workspace, oldest first.
    pub async fn list_operations(&self, ws: &WorkspaceId, limit: usize) -> Result<Vec<Operation>> {
        let rows = self
            .meta()
            .scan(
                KeyRange::prefix(keys::operation_prefix(&self.id, ws))
                    .reverse()
                    .limit(limit),
            )
            .await?;
        let mut ops: Vec<Operation> = rows
            .iter()
            .map(|(_, v)| decode_record(v))
            .collect::<Result<_>>()?;
        ops.reverse();
        Ok(ops)
    }

    // ---- intents ---------------------------------------------------------

    pub async fn get_intent(&self, id: &IntentId) -> Result<Option<Intent>> {
        match self.meta().get(&keys::intent(&self.id, id)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn put_intent(&self, intent: &Intent) -> Result<()> {
        self.meta()
            .set(keys::intent(&self.id, &intent.id), encode_record(intent)?)
            .await
    }

    pub async fn list_intents(&self) -> Result<Vec<Intent>> {
        let rows = self
            .meta()
            .scan(KeyRange::prefix(keys::intent_prefix(&self.id)))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    // ---- derived data ----------------------------------------------------

    pub async fn get_derived(&self, kind: &str, hash: &ObjectHash) -> Result<Option<Bytes>> {
        self.meta().get(&keys::derived(&self.id, kind, hash)).await
    }

    pub async fn put_derived(&self, kind: &str, hash: &ObjectHash, value: Bytes) -> Result<()> {
        self.meta()
            .set(keys::derived(&self.id, kind, hash), value)
            .await
    }

    // ---- relationships (ReBAC tuples) ------------------------------------

    pub async fn add_relation(&self, object: &str, relation: &str, subject: &str) -> Result<()> {
        self.meta()
            .set(
                keys::rebac(&self.id, object, relation, subject),
                Bytes::new(),
            )
            .await
    }

    pub async fn remove_relation(&self, object: &str, relation: &str, subject: &str) -> Result<()> {
        self.meta()
            .transact(Transaction::new().delete(keys::rebac(&self.id, object, relation, subject)))
            .await?;
        Ok(())
    }

    /// All `(relation, subject)` tuples on an object.
    pub async fn relations_of(&self, object: &str) -> Result<Vec<(String, String)>> {
        let prefix = keys::rebac_object_prefix(&self.id, object);
        let rows = self.meta().scan(KeyRange::prefix(prefix.clone())).await?;
        rows.iter()
            .map(|(k, _)| {
                let tail = std::str::from_utf8(&k[prefix.len()..])
                    .map_err(|e| Error::Codec(e.to_string()))?;
                let (rel, subj) = tail
                    .split_once('/')
                    .ok_or_else(|| Error::Codec("bad relation key".into()))?;
                Ok((rel.to_string(), subj.to_string()))
            })
            .collect()
    }

    // ---- misc ------------------------------------------------------------

    pub async fn get_misc(&self, name: &str) -> Result<Option<Bytes>> {
        self.meta().get(&keys::misc(&self.id, name)).await
    }

    pub async fn set_misc(&self, name: &str, value: Bytes) -> Result<()> {
        self.meta().set(keys::misc(&self.id, name), value).await
    }

    /// Fetch raw object bytes by kind and hash.
    pub async fn get_bytes(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        self.objects().get(kind, hash).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{Principal, Provenance};
    use std::collections::BTreeMap;

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
        assert_eq!(repo.log_head().await.unwrap(), 2);
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

    #[tokio::test]
    async fn repo_names_are_unique() {
        let stores = Stores::memory();
        stores.create_repo("r", "").await.unwrap();
        assert!(stores.create_repo("r", "").await.unwrap_err().is_conflict());
        assert!(stores.get_repo_by_name("r").await.unwrap().is_some());
        assert_eq!(stores.list_repos().await.unwrap().len(), 1);
    }
}

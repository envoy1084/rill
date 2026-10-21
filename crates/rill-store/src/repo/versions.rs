//! Change versions, the version log and signatures.

use rill_types::{
    ChangeId, ChangeInfo, ChangeVersion, ChangeVersionHash, Encodable, EventKind, Signature,
    Versionstamp,
};

use super::codec::{
    decode_record, decode_tagged, encode_record, encode_tagged, hash_value, parse_hash,
};
use super::store::RepoStore;
use crate::error::{Error, Result};
use crate::meta::{prefix_end, stamp_from_key, KeyRange, Transaction};
use crate::object::ObjectStoreExt;

impl RepoStore {
    /// Store a change version: the object itself, its info projection, an
    /// entry in the version log and in the change's version list.
    /// Idempotent: storing the same version twice, even concurrently, adds
    /// it to the logs once.
    pub async fn put_change_version(&self, cv: &ChangeVersion) -> Result<ChangeVersionHash> {
        let hash = cv.hash();
        if self.has_change_version(&hash).await? {
            return Ok(hash);
        }
        let mut tx = self.stage_version(cv).await?;
        tx.mutations.extend(self.event_by(
            &cv.author,
            EventKind::VersionStored {
                version: hash,
                change: cv.change_id,
            },
        )?);
        match self.meta().transact(tx).await {
            Ok(_) => Ok(hash),
            Err(Error::Conflict(_)) if self.has_change_version(&hash).await? => Ok(hash),
            Err(e) => Err(e),
        }
    }

    /// Store `cv`'s object and return the transaction that makes it
    /// visible: its info row, version-log entry and place in its change's
    /// version list, conditioned on the version being new. Callers add
    /// what else must commit with it.
    pub async fn stage_version(&self, cv: &ChangeVersion) -> Result<Transaction> {
        let hash = self.objects().put_object(cv).await?;
        let info = ChangeInfo::from_version(cv, None);
        let info_key = self.keys().change_info(&hash);
        Ok(Transaction::new()
            .expect_absent(info_key.clone())
            .set(info_key, encode_record(&info)?)
            .append_stamped(self.keys().log(), hash_value(&hash))
            .append_log(
                self.keys().change_versions(&cv.change_id),
                hash_value(&hash),
            ))
    }

    /// The mutation storing `signatures` as all of `hash`'s signatures.
    pub fn signatures_mutation(
        &self,
        hash: &ChangeVersionHash,
        signatures: &[Signature],
    ) -> Result<crate::meta::Mutation> {
        Ok(crate::meta::Mutation::Set {
            key: self.keys().signatures(hash),
            value: encode_tagged("signatures", signatures)?,
        })
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
        match self.meta().get(&self.keys().change_info(hash)).await? {
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
            .get(&self.keys().change_info(hash))
            .await?
            .is_some())
    }

    /// Versions of a change in creation order.
    pub async fn change_versions(&self, id: &ChangeId) -> Result<Vec<ChangeVersionHash>> {
        let rows = self
            .meta()
            .scan(KeyRange::prefix(self.keys().change_versions(id)))
            .await?;
        rows.iter().map(|(_, v)| parse_hash(v)).collect()
    }

    /// Version-log entries after `after` (from the start when `None`),
    /// oldest first. Each entry's stamp resumes the log after it.
    pub async fn log_after(
        &self,
        after: Option<Versionstamp>,
        limit: usize,
    ) -> Result<Vec<(Versionstamp, ChangeVersionHash)>> {
        let prefix = self.keys().log();
        let end = prefix_end(&prefix).expect("a log prefix is not all 0xFF");
        let range = match after {
            Some(stamp) => KeyRange {
                start: std::ops::Bound::Excluded(self.keys().log_entry(&stamp)),
                end: std::ops::Bound::Excluded(end),
                limit,
                reverse: false,
            },
            None => KeyRange::prefix(prefix.clone()).limit(limit),
        };
        let rows = self.meta().scan(range).await?;
        rows.iter()
            .map(|(k, v)| {
                let stamp = stamp_from_key(prefix.len(), k)
                    .ok_or_else(|| Error::Codec("bad log key".into()))?;
                Ok((stamp, parse_hash(v)?))
            })
            .collect()
    }

    /// The stamp of the newest version-log entry.
    pub async fn log_head(&self) -> Result<Option<Versionstamp>> {
        let prefix = self.keys().log();
        let rows = self
            .meta()
            .scan(KeyRange::prefix(prefix.clone()).reverse().limit(1))
            .await?;
        Ok(rows
            .first()
            .and_then(|(k, _)| stamp_from_key(prefix.len(), k)))
    }

    pub async fn put_signatures(
        &self,
        hash: &ChangeVersionHash,
        signatures: &[Signature],
    ) -> Result<()> {
        self.meta()
            .set(
                self.keys().signatures(hash),
                encode_tagged("signatures", signatures)?,
            )
            .await
    }

    /// Append signatures to the ones already stored for `hash`.
    pub async fn add_signatures(
        &self,
        hash: &ChangeVersionHash,
        signatures: &[Signature],
    ) -> Result<()> {
        let key = self.keys().signatures(hash);
        let current = self.meta().get(&key).await?;
        let mut all: Vec<Signature> = match &current {
            Some(bytes) => decode_tagged("signatures", bytes)?,
            None => Vec::new(),
        };
        all.extend_from_slice(signatures);
        let tx = Transaction::new()
            .expect(key.clone(), current)
            .set(key, encode_tagged("signatures", &all)?);
        self.meta().transact(tx).await?;
        Ok(())
    }

    pub async fn get_signatures(&self, hash: &ChangeVersionHash) -> Result<Vec<Signature>> {
        match self.meta().get(&self.keys().signatures(hash)).await? {
            Some(bytes) => decode_tagged("signatures", &bytes),
            None => Ok(Vec::new()),
        }
    }
}

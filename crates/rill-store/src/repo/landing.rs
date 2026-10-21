//! Moving trunk: one transaction per batch of landings.
//!
//! A batch writes, all or nothing: the trunk positions (gap-free, after
//! the head it expects), the trunk-head row the compare-and-swap is on, a
//! landing record per position, each change's review marked `Landed`,
//! events, quota charges, the trunk ref and whatever the caller adds (the
//! jobs a landing makes necessary). A lander passes the lease it holds; a
//! lander that was replaced cannot land, because the epoch moved on.

use bytes::Bytes;
use rill_types::{
    ChangeId, ChangeVersionHash, Event, EventKind, Landing, Principal, RefName, ReviewStatus,
    RiskClass, Timestamp, TrunkSeq,
};

use super::codec::{decode_record, encode_record, hash_value};
use super::quota::metric;
use super::reviews::ReviewSnapshot;
use super::store::RepoStore;
use super::trunk::{head_row, parse_head};
use crate::error::{Error, Result};
use crate::meta::{seq_from_key, KeyRange, Transaction};

/// One version to land.
#[derive(Debug, Clone)]
pub struct LandEntry {
    pub version: ChangeVersionHash,
    pub change_id: ChangeId,
    /// The version that was asked to land, before any rebase.
    pub requested_version: ChangeVersionHash,
    pub requester: Principal,
    pub risk: RiskClass,
    /// The change's review as read; landing marks it `Landed`. `None` for
    /// a change landed without review.
    pub review: Option<ReviewSnapshot>,
}

/// Versions to land together, in order.
#[derive(Debug, Clone, Default)]
pub struct LandBatch {
    /// The trunk head the batch lands on; `None` for an empty trunk.
    pub expected: Option<ChangeVersionHash>,
    pub entries: Vec<LandEntry>,
    /// The lease the lander holds; the land fails once it is lost.
    pub fence: Option<super::leases::Lease>,
    /// Who lands the batch, for its events.
    pub actor: Option<Principal>,
    /// More writes that commit with the landing, such as the jobs it
    /// makes necessary.
    pub also: Transaction,
}

impl RepoStore {
    /// Land `batch` in one transaction and return its landing records.
    /// Fails with a conflict when trunk moved, the lease moved on, or a
    /// review changed since it was read.
    pub async fn land(&self, batch: LandBatch) -> Result<Vec<Landing>> {
        let keys = self.keys();
        let head_key = keys.trunk_head();
        let head = self.meta().get(&head_key).await?;
        let (head_seq, head_hash) = match &head {
            Some(row) => {
                let (seq, hash) = parse_head(row)?;
                (seq.get(), Some(hash))
            }
            None => (0, None),
        };
        if head_hash != batch.expected {
            return Err(Error::Conflict("trunk moved".into()));
        }
        let Some(last) = batch.entries.last() else {
            return Ok(Vec::new());
        };
        let now = Timestamp::now();
        let ref_key = keys.reference(&RefName::trunk());
        let mut tx = Transaction::new()
            .expect(head_key.clone(), head)
            .expect(ref_key.clone(), batch.expected.as_ref().map(hash_value))
            .set(
                head_key,
                head_row(head_seq + batch.entries.len() as u64, &last.version),
            )
            .set(ref_key, hash_value(&last.version));
        if let Some(lease) = &batch.fence {
            tx.conditions.push(lease.fence(self));
        }
        tx = tx.merge(batch.also);
        let stores = self.stores();
        let mut landings = Vec::with_capacity(batch.entries.len());
        for (i, entry) in batch.entries.into_iter().enumerate() {
            let seq = TrunkSeq(head_seq + 1 + i as u64);
            let landing = Landing {
                seq,
                change_id: entry.change_id,
                version: entry.version,
                requested_version: entry.requested_version,
                requester: entry.requester.clone(),
                risk: entry.risk,
                landed_at: now,
            };
            let actor = batch
                .actor
                .clone()
                .unwrap_or_else(|| entry.requester.clone());
            let event = |kind| Event {
                kind,
                actor: actor.clone(),
                at: now,
            };
            tx = tx
                .expect_absent(keys.trunk(seq.get()))
                .set(keys.trunk(seq.get()), hash_value(&entry.version))
                .set(
                    keys.trunk_by_hash(&entry.version),
                    Bytes::copy_from_slice(&seq.get().to_be_bytes()),
                )
                .set(keys.landing(seq.get()), encode_record(&landing)?);
            tx.mutations.extend(self.event(&event(EventKind::Landed {
                seq,
                version: entry.version,
                change: entry.change_id,
            }))?);
            tx.mutations
                .push(stores.charge(&entry.requester, metric::LANDINGS, 1));
            if let Some(snapshot) = &entry.review {
                let from = snapshot.state.status;
                if !matches!(from, ReviewStatus::Landing | ReviewStatus::Landable) {
                    return Err(Error::Invalid(format!(
                        "change {} is {from:?}, not landable",
                        entry.change_id
                    )));
                }
                let mut next = snapshot.state.clone();
                next.status = ReviewStatus::Landed;
                next.landed_seq = Some(seq);
                next.updated_at = now.max(snapshot.state.updated_at);
                tx = tx.merge(self.review_writes(snapshot, &next, &actor)?);
            }
            landings.push(landing);
        }
        self.meta().transact(tx).await?;
        Ok(landings)
    }

    /// Advance trunk to `hash` if it currently points at `expected`, for
    /// callers without review or a lease (imports, tools, tests).
    pub async fn land_on_trunk(
        &self,
        expected: Option<ChangeVersionHash>,
        hash: ChangeVersionHash,
    ) -> Result<TrunkSeq> {
        let cv = self.get_change_version_required(&hash).await?;
        let landed = self
            .land(LandBatch {
                expected,
                entries: vec![LandEntry {
                    version: hash,
                    change_id: cv.change_id,
                    requested_version: hash,
                    requester: cv.author,
                    risk: RiskClass::Normal,
                    review: None,
                }],
                ..LandBatch::default()
            })
            .await?;
        Ok(landed[0].seq)
    }

    /// The landing record at `seq`.
    pub async fn landing(&self, seq: TrunkSeq) -> Result<Option<Landing>> {
        match self.meta().get(&self.keys().landing(seq.get())).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    /// The landing record of `version`, if it is on trunk.
    pub async fn landing_of(&self, version: &ChangeVersionHash) -> Result<Option<Landing>> {
        match self.trunk_seq_of(version).await? {
            Some(seq) => self.landing(seq).await,
            None => Ok(None),
        }
    }

    /// The newest `limit` landing records, oldest first.
    pub async fn list_landings(&self, limit: usize) -> Result<Vec<Landing>> {
        let prefix = self.keys().landings();
        let mut rows = self
            .meta()
            .scan(KeyRange::prefix(prefix.clone()).reverse().limit(limit))
            .await?;
        rows.reverse();
        rows.iter()
            .map(|(k, v)| {
                seq_from_key(prefix.len(), k)
                    .ok_or_else(|| Error::Codec("bad landing key".into()))?;
                decode_record(v)
            })
            .collect()
    }
}

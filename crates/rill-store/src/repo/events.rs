//! The repository's event log, written by the transactions that cause the
//! events.

use rill_types::{Event, EventKind, Principal, Timestamp, Versionstamp};

use super::codec::{decode_record, encode_record};
use super::store::RepoStore;
use crate::error::{Error, Result};
use crate::keys::table;
use crate::meta::{prefix_end, stamp_from_key, KeyRange, Mutation};

impl RepoStore {
    /// The mutations recording `event`, for the transaction that causes
    /// it: the stamped event row and a bump of the event counter tailers
    /// wait on.
    pub fn event(&self, event: &Event) -> Result<[Mutation; 2]> {
        Ok([
            Mutation::AppendStamped {
                prefix: self.keys().table(table::EVENT),
                value: encode_record(event)?,
            },
            Mutation::Add {
                key: self.keys().table(table::EVENT_COUNT),
                delta: 1,
            },
        ])
    }

    /// [`RepoStore::event`] for `kind`, caused by `actor` now.
    pub fn event_by(&self, actor: &Principal, kind: EventKind) -> Result<[Mutation; 2]> {
        self.event(&Event {
            kind,
            actor: actor.clone(),
            at: Timestamp::now(),
        })
    }

    /// Events written so far.
    pub async fn event_count(&self) -> Result<i64> {
        self.meta()
            .counter(&self.keys().table(table::EVENT_COUNT))
            .await
    }

    /// Wait until the event count differs from `seen` or `timeout` passes;
    /// returns the count.
    pub async fn wait_for_events(&self, seen: i64, timeout: std::time::Duration) -> Result<i64> {
        self.meta()
            .wait_counter(&self.keys().table(table::EVENT_COUNT), seen, timeout)
            .await
    }

    /// Delete events older than `before`, oldest first, up to `limit`;
    /// returns how many.
    pub async fn sweep_events(&self, before: Timestamp, limit: usize) -> Result<usize> {
        let prefix = self.keys().table(table::EVENT);
        let rows = self
            .meta()
            .scan(KeyRange::prefix(prefix).limit(limit))
            .await?;
        let mut tx = crate::meta::Transaction::new();
        let mut removed = 0;
        for (key, value) in rows {
            let event: Event = decode_record(&value)?;
            if event.at >= before {
                break;
            }
            tx = tx.delete(key);
            removed += 1;
        }
        if removed > 0 {
            self.meta().transact(tx).await?;
        }
        Ok(removed)
    }

    /// Events after `after` (from the start when `None`), oldest first.
    pub async fn events_after(
        &self,
        after: Option<Versionstamp>,
        limit: usize,
    ) -> Result<Vec<(Versionstamp, Event)>> {
        let prefix = self.keys().table(table::EVENT);
        let range = match after {
            Some(stamp) => KeyRange {
                start: std::ops::Bound::Excluded(crate::meta::stamp_key(&prefix, &stamp)),
                end: std::ops::Bound::Excluded(prefix_end(&prefix).expect("a table prefix ends")),
                limit,
                reverse: false,
            },
            None => KeyRange::prefix(prefix.clone()).limit(limit),
        };
        self.meta()
            .scan(range)
            .await?
            .iter()
            .map(|(k, v)| {
                let stamp = stamp_from_key(prefix.len(), k)
                    .ok_or_else(|| Error::Codec("bad event key".into()))?;
                Ok((stamp, decode_record(v)?))
            })
            .collect()
    }
}

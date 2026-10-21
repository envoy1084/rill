//! The identity change log. Every change that other servers must act on
//! (a revocation, a new key) appends an entry in the transaction that
//! makes it and bumps a counter followers wait on.

use rill_types::identity::{IdentityChange, IdentityEvent};
use rill_types::{Principal, Timestamp, Versionstamp};

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::{prefix_end, stamp_from_key, stamp_key, KeyRange, Mutation, Transaction};
use crate::repo::{decode_record, encode_record};

impl IdentityStore {
    /// The mutations logging `change` by `actor`, for the transaction that
    /// makes it.
    pub fn log(&self, change: IdentityChange, actor: &Principal) -> Result<[Mutation; 2]> {
        let event = IdentityEvent {
            change,
            actor: actor.clone(),
            at: Timestamp::now(),
        };
        Ok([
            Mutation::AppendStamped {
                prefix: keys::identity_table(table::IDENT_LOG),
                value: encode_record(&event)?,
            },
            Mutation::Add {
                key: keys::identity_table(table::IDENT_LOG_COUNT),
                delta: 1,
            },
        ])
    }

    /// `tx` with `change` logged.
    pub(super) fn logged(
        &self,
        mut tx: Transaction,
        change: IdentityChange,
        actor: &Principal,
    ) -> Result<Transaction> {
        tx.mutations.extend(self.log(change, actor)?);
        Ok(tx)
    }

    /// Record `change` by `actor` on its own: for actions that change no
    /// stored record, such as issuing a token.
    pub async fn record(&self, change: IdentityChange, actor: &Principal) -> Result<()> {
        let tx = self.logged(Transaction::new(), change, actor)?;
        self.meta.transact(tx).await?;
        Ok(())
    }

    /// Entries written so far.
    pub async fn log_count(&self) -> Result<i64> {
        self.meta
            .counter(&keys::identity_table(table::IDENT_LOG_COUNT))
            .await
    }

    /// Wait until the entry count differs from `seen` or `timeout` passes;
    /// returns the count.
    pub async fn wait_for_log(&self, seen: i64, timeout: std::time::Duration) -> Result<i64> {
        self.meta
            .wait_counter(&keys::identity_table(table::IDENT_LOG_COUNT), seen, timeout)
            .await
    }

    /// The cursor of the newest entry, if any.
    pub async fn latest_log_entry(&self) -> Result<Option<Versionstamp>> {
        let prefix = keys::identity_table(table::IDENT_LOG);
        let rows = self
            .meta
            .scan(KeyRange::prefix(prefix.clone()).reverse().limit(1))
            .await?;
        rows.first()
            .map(|(k, _)| {
                stamp_from_key(prefix.len(), k)
                    .ok_or_else(|| Error::Codec("bad identity log key".into()))
            })
            .transpose()
    }

    /// Entries after `after` (from the start when `None`), oldest first.
    pub async fn log_after(
        &self,
        after: Option<Versionstamp>,
        limit: usize,
    ) -> Result<Vec<(Versionstamp, IdentityEvent)>> {
        let prefix = keys::identity_table(table::IDENT_LOG);
        let range = match after {
            Some(stamp) => KeyRange {
                start: std::ops::Bound::Excluded(stamp_key(&prefix, &stamp)),
                end: std::ops::Bound::Excluded(prefix_end(&prefix).expect("a table prefix ends")),
                limit,
                reverse: false,
            },
            None => KeyRange::prefix(prefix.clone()).limit(limit),
        };
        self.meta
            .scan(range)
            .await?
            .iter()
            .map(|(k, v)| {
                let stamp = stamp_from_key(prefix.len(), k)
                    .ok_or_else(|| Error::Codec("bad identity log key".into()))?;
                Ok((stamp, decode_record(v)?))
            })
            .collect()
    }

    /// Delete entries older than `before`, up to `limit`; returns how many.
    pub async fn sweep_log(&self, before: Timestamp, limit: usize) -> Result<usize> {
        let prefix = keys::identity_table(table::IDENT_LOG);
        let rows = self
            .meta
            .scan(KeyRange::prefix(prefix).limit(limit))
            .await?;
        let mut tx = Transaction::new();
        let mut removed = 0;
        for (key, value) in rows {
            let event: IdentityEvent = decode_record(&value)?;
            if event.at >= before {
                break;
            }
            tx = tx.delete(key);
            removed += 1;
        }
        if removed > 0 {
            self.meta.transact(tx).await?;
        }
        Ok(removed)
    }
}

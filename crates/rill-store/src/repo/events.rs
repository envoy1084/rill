//! The repository's event log, written by the transactions that cause the
//! events.

use rill_types::{Event, Versionstamp};

use super::codec::{decode_record, encode_record};
use super::store::RepoStore;
use crate::error::{Error, Result};
use crate::keys::table;
use crate::meta::{prefix_end, stamp_from_key, KeyRange, Mutation};

impl RepoStore {
    /// The mutation recording `event`, for the transaction that causes it.
    pub fn event(&self, event: &Event) -> Result<Mutation> {
        Ok(Mutation::AppendStamped {
            prefix: self.keys().table(table::EVENT),
            value: encode_record(event)?,
        })
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

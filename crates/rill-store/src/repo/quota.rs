//! Usage counters for quotas, split across shards so that many writers
//! charging one principal never conflict.

use rill_types::{Principal, Timestamp};

use super::stores::Stores;
use crate::error::Result;
use crate::keys;
use crate::meta::Mutation;

/// Shards per counter.
pub const QUOTA_SHARDS: u8 = 16;

/// Metrics counted per principal per day.
pub mod metric {
    /// Change versions stored.
    pub const VERSIONS: &str = "versions";
    /// Versions landed on trunk.
    pub const LANDINGS: &str = "landings";
}

/// The UTC day a timestamp falls on, counted from the Unix epoch.
pub fn day_of(at: Timestamp) -> i64 {
    at.millis().div_euclid(86_400_000)
}

impl Stores {
    /// A mutation adding `amount` to today's `metric` for `principal`, to
    /// include in the transaction doing the work.
    pub fn charge(&self, principal: &Principal, metric: &str, amount: i64) -> Mutation {
        let shard = rill_types::ids::random_id()[0] % QUOTA_SHARDS;
        Mutation::Add {
            key: keys::quota(
                self.namespace(),
                &principal.id.to_string(),
                metric,
                day_of(Timestamp::now()),
                shard,
            ),
            delta: amount,
        }
    }

    /// `principal`'s `metric` on `day`, summed over shards.
    pub async fn usage(&self, principal: &Principal, metric: &str, day: i64) -> Result<i64> {
        let id = principal.id.to_string();
        let mut total = 0;
        for shard in 0..QUOTA_SHARDS {
            total += self
                .meta
                .counter(&keys::quota(self.namespace(), &id, metric, day, shard))
                .await?;
        }
        Ok(total)
    }
}

//! Quarantined targets: tests and steps whose failures neither block a
//! landing nor make trunk red, because they fail and pass on the same
//! inputs. A quarantine lasts until an owner lifts it.

use rill_store::repo::{decode_record, encode_record};
use rill_store::{KeyRange, Transaction};
use rill_types::{EventKind, IntentId, Principal, Timestamp};
use serde::{Deserialize, Serialize};

use super::store::TestHistory;
use crate::error::Result;

/// A target taken out of the verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quarantine {
    /// `step:<name>[#shard]` or `test:<id>`.
    pub target: String,
    pub since: Timestamp,
    pub by: Principal,
    pub reason: String,
    /// The intent tracking the fix, when one was opened.
    pub intent: Option<IntentId>,
}

impl rill_types::Record for Quarantine {
    const TAG: &'static str = "test-quarantine";
}

impl TestHistory {
    /// Quarantine a target; an existing quarantine is kept. Returns
    /// whether it is new.
    pub async fn quarantine(&self, quarantine: &Quarantine) -> Result<bool> {
        let key = self.repo().keys().test_quarantine(&quarantine.target);
        if self.repo().meta().get(&key).await?.is_some() {
            return Ok(false);
        }
        let mut tx = Transaction::new()
            .expect_absent(key.clone())
            .set(key, encode_record(quarantine)?);
        tx.mutations.extend(self.repo().event_by(
            &quarantine.by,
            EventKind::QuarantineChanged {
                target: quarantine.target.clone(),
                quarantined: true,
            },
        )?);
        match self.repo().meta().transact(tx).await {
            Ok(_) => Ok(true),
            Err(e) if e.is_conflict() => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Lift the quarantine of `target`, by `by`; returns whether there
    /// was one.
    pub async fn release(&self, target: &str, by: &Principal) -> Result<bool> {
        let key = self.repo().keys().test_quarantine(target);
        let Some(current) = self.repo().meta().get(&key).await? else {
            return Ok(false);
        };
        let mut tx = Transaction::new()
            .expect(key.clone(), Some(current))
            .delete(key);
        tx.mutations.extend(self.repo().event_by(
            by,
            EventKind::QuarantineChanged {
                target: target.to_string(),
                quarantined: false,
            },
        )?);
        match self.repo().meta().transact(tx).await {
            Ok(_) => Ok(true),
            Err(e) if e.is_conflict() => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Every quarantined target.
    pub async fn quarantined(&self) -> Result<Vec<Quarantine>> {
        let rows = self
            .repo()
            .meta()
            .scan(KeyRange::prefix(self.repo().keys().all_test_quarantine()))
            .await?;
        rows.into_iter()
            .map(|(_, value)| Ok(decode_record(&value)?))
            .collect()
    }
}

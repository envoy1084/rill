//! Trunk health and the incidents behind it.

use bytes::Bytes;
use rill_types::{Event, EventKind, Incident, Principal, Timestamp, TrunkHealth, TrunkSeq};

use super::codec::{decode_record, encode_record};
use super::store::RepoStore;
use crate::error::Result;
use crate::meta::{KeyRange, Transaction};

/// Misc row holding the [`TrunkHealth`].
pub const TRUNK_HEALTH_MISC: &str = "trunk_health";

impl RepoStore {
    /// Trunk's health; green when never judged.
    pub async fn trunk_health(&self) -> Result<TrunkHealth> {
        match self.get_misc(TRUNK_HEALTH_MISC).await? {
            Some(bytes) => decode_record(&bytes),
            None => Ok(TrunkHealth::default()),
        }
    }

    /// Record trunk's health, by `actor`.
    pub async fn set_trunk_health(&self, health: &TrunkHealth, actor: &Principal) -> Result<()> {
        let event = Event {
            kind: EventKind::TrunkHealthChanged {
                state: health.state.as_str().to_string(),
                frozen: health.frozen,
            },
            actor: actor.clone(),
            at: Timestamp::now(),
        };
        self.set_misc_with_event(
            TRUNK_HEALTH_MISC,
            Bytes::from(encode_record(health)?.to_vec()),
            &event,
        )
        .await
    }

    /// Store `incident`, by `actor`.
    pub async fn put_incident(&self, incident: &Incident, actor: &Principal) -> Result<()> {
        let mut tx = Transaction::new().set(
            self.keys().incident(incident.culprit.get()),
            encode_record(incident)?,
        );
        tx.mutations.extend(self.event_by(
            actor,
            EventKind::IncidentChanged {
                incident: incident.id(),
                culprit: incident.culprit,
                state: incident.state.as_str().to_string(),
            },
        )?);
        self.meta().transact(tx).await?;
        Ok(())
    }

    /// The incident whose culprit is `culprit`.
    pub async fn incident(&self, culprit: TrunkSeq) -> Result<Option<Incident>> {
        match self
            .meta()
            .get(&self.keys().incident(culprit.get()))
            .await?
        {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    /// The newest `limit` incidents, newest culprit first.
    pub async fn incidents(&self, limit: usize) -> Result<Vec<Incident>> {
        let rows = self
            .meta()
            .scan(
                KeyRange::prefix(self.keys().incidents())
                    .reverse()
                    .limit(limit),
            )
            .await?;
        rows.into_iter()
            .map(|(_, value)| decode_record(&value))
            .collect()
    }
}

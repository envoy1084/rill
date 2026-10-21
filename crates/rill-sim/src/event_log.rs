//! The event log against the state it records: run once the system
//! settles, with faults off.

use std::collections::BTreeMap;

use rill_store::RepoStore;
use rill_types::{ChangeVersionHash, EventKind, TrunkSeq};

use crate::oracle::{Invariant, Oracle};

/// More than any run writes.
const ALL: usize = 1_000_000;

impl Oracle {
    /// Every stored version has exactly one `VersionStored` event and
    /// every trunk position exactly one `Landed` event, with nothing extra,
    /// and the event counter matches the log.
    pub async fn check_event_log(&mut self, repo: &RepoStore) {
        let (Ok(events), Ok(log), Ok(count)) = (
            repo.events_after(None, ALL).await,
            repo.log_after(None, ALL).await,
            repo.event_count().await,
        ) else {
            return;
        };
        if count != events.len() as i64 {
            self.violate(
                Invariant::EventsMatchState,
                format!("counter says {count} events, the log has {}", events.len()),
            );
        }
        let mut stored: BTreeMap<ChangeVersionHash, i64> = BTreeMap::new();
        let mut landed: BTreeMap<TrunkSeq, Vec<ChangeVersionHash>> = BTreeMap::new();
        for (_, event) in &events {
            match &event.kind {
                EventKind::VersionStored { version, .. } => {
                    *stored.entry(*version).or_default() += 1
                }
                EventKind::Landed { seq, version, .. } => {
                    landed.entry(*seq).or_default().push(*version)
                }
                _ => {}
            }
        }
        for (_, version) in &log {
            *stored.entry(*version).or_default() -= 1;
        }
        for (version, surplus) in stored.into_iter().filter(|(_, n)| *n != 0) {
            self.violate(
                Invariant::EventsMatchState,
                format!(
                    "version {} has {surplus:+} events more than log entries",
                    version.short()
                ),
            );
        }
        let trunk = self.trunk().to_vec();
        for (i, version) in trunk.iter().enumerate() {
            let seq = TrunkSeq(i as u64 + 1);
            match landed.remove(&seq).as_deref() {
                Some([one]) if one == version => {}
                other => self.violate(
                    Invariant::EventsMatchState,
                    format!("trunk@{} has landed events {other:?}", seq.get()),
                ),
            }
        }
        for seq in landed.keys() {
            self.violate(
                Invariant::EventsMatchState,
                format!("landed event for trunk@{} past the head", seq.get()),
            );
        }
    }
}

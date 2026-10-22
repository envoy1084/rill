//! The current snapshot of one repository's tuples, replaced when the
//! store's relation version moves.

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rill_store::RepoStore;

use super::snapshot::Snapshot;
use crate::error::Result;

/// How long a snapshot is trusted without a follower having confirmed it;
/// bounds staleness if the follower stalls.
pub const DEFAULT_SNAPSHOT_TTL: Duration = Duration::from_secs(30);

/// Holds one snapshot. A snapshot is served while it is younger than the
/// TTL and at least as new as the caller asks; otherwise it is reloaded.
#[derive(Debug)]
pub struct SnapshotCache {
    ttl: Duration,
    current: Mutex<Option<(Arc<Snapshot>, Instant)>>,
}

impl SnapshotCache {
    /// A cache trusting snapshots for `ttl`.
    pub fn new(ttl: Duration) -> Self {
        SnapshotCache {
            ttl,
            current: Mutex::new(None),
        }
    }

    /// A snapshot reflecting at least relation version `at_least`.
    pub async fn get(&self, repo: &RepoStore, at_least: i64) -> Result<Arc<Snapshot>> {
        if let Some(snapshot) = self.peek(at_least) {
            return Ok(snapshot);
        }
        let fresh = Arc::new(Snapshot::load(repo).await?);
        let mut current = self.current.lock();
        let newer = current
            .as_ref()
            .is_none_or(|(held, _)| held.version() <= fresh.version());
        if newer {
            *current = Some((fresh.clone(), Instant::now()));
        }
        Ok(fresh)
    }

    /// The held snapshot's version, if any.
    pub fn version(&self) -> Option<i64> {
        self.current.lock().as_ref().map(|(s, _)| s.version())
    }

    /// Drop the held snapshot so the next check reloads.
    pub fn invalidate(&self) {
        *self.current.lock() = None;
    }

    fn peek(&self, at_least: i64) -> Option<Arc<Snapshot>> {
        let current = self.current.lock();
        current
            .as_ref()
            .filter(|(s, at)| s.version() >= at_least && at.elapsed() <= self.ttl)
            .map(|(s, _)| s.clone())
    }
}

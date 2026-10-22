//! Roots this process has asked the search writer to index and is still
//! waiting for. Concurrent searches of one unindexed root (every search of
//! a new trunk head, under load) share one index job instead of each
//! queueing their own.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use rill_types::TreeHash;

/// The roots with an index request in flight.
#[derive(Default)]
pub(crate) struct IndexRequests {
    roots: Arc<Mutex<HashSet<TreeHash>>>,
}

/// Held by the search that queued the job for its root; dropping it, when
/// the search finishes or is cancelled, lets the next search ask again.
pub(crate) struct IndexRequest {
    roots: Arc<Mutex<HashSet<TreeHash>>>,
    root: TreeHash,
}

impl IndexRequests {
    /// Claim the request for `root`, or `None` when another search holds it.
    pub(crate) fn claim(&self, root: TreeHash) -> Option<IndexRequest> {
        let mut roots = self.roots.lock().unwrap_or_else(|e| e.into_inner());
        roots.insert(root).then(|| IndexRequest {
            roots: self.roots.clone(),
            root,
        })
    }
}

impl Drop for IndexRequest {
    fn drop(&mut self) {
        let mut roots = self.roots.lock().unwrap_or_else(|e| e.into_inner());
        roots.remove(&self.root);
    }
}

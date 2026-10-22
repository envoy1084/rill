//! Read grants the server returned, by the object hashes they name. A
//! listing, path resolution, diff or version read returns a grant for the
//! objects it reveals; fetching one of them by hash sends that grant.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rill_types::{ContentRef, EntryKind, ObjectHash};

/// How long a recorded grant is used; servers issue them for ten minutes.
const KEEP: Duration = Duration::from_secs(9 * 60);
/// Recorded hashes past which expired ones are dropped.
const MAX_ENTRIES: usize = 200_000;

/// A grant and when to stop sending it.
type Held = (Arc<str>, Instant);

/// Grants by hash, shared by a client's clones.
#[derive(Clone, Debug, Default)]
pub struct ReadGrants(Arc<Mutex<HashMap<ObjectHash, Held>>>);

impl ReadGrants {
    /// Remember `grant` for each of `hashes`.
    pub fn record(&self, grant: &str, hashes: impl IntoIterator<Item = ObjectHash>) {
        if grant.is_empty() {
            return;
        }
        let grant: Arc<str> = Arc::from(grant);
        let until = Instant::now() + KEEP;
        let mut map = self.0.lock();
        if map.len() > MAX_ENTRIES {
            let now = Instant::now();
            map.retain(|_, (_, t)| *t > now);
            if map.len() > MAX_ENTRIES {
                map.clear();
            }
        }
        for hash in hashes {
            map.insert(hash, (grant.clone(), until));
        }
    }

    /// The grant to send when fetching `hash`; empty when none is known.
    pub fn for_hash(&self, hash: &ObjectHash) -> String {
        let map = self.0.lock();
        match map.get(hash) {
            Some((grant, until)) if *until > Instant::now() => grant.to_string(),
            _ => String::new(),
        }
    }
}

/// The object a content reference is fetched by.
pub(crate) fn content_hash(content: &ContentRef) -> Option<ObjectHash> {
    match content {
        ContentRef::Inline { .. } => None,
        ContentRef::Blob { hash, .. } | ContentRef::Chunked { hash, .. } => Some(*hash),
    }
}

/// The object a tree entry names.
pub(crate) fn entry_hash(kind: &EntryKind) -> Option<ObjectHash> {
    match kind {
        EntryKind::File { content, .. } => content_hash(content),
        EntryKind::Dir { hash, .. } => Some(*hash),
    }
}

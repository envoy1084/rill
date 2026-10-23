//! The overlay in memory: the latest operation per path, with sequence
//! numbers that say what is still to be synced. Pure; the state store
//! persists every change before it is applied here.

use std::collections::BTreeMap;
use std::ops::Bound;

use rill_types::{RepoPath, RepoPathBuf};

use super::record::{OverlayEntry, OverlayOp};

/// A mount's local edits relative to its base.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overlay {
    entries: BTreeMap<RepoPathBuf, OverlayEntry>,
    next_seq: u64,
    synced: u64,
}

impl Overlay {
    /// An overlay restored from stored entries, with everything up to
    /// `synced` already pushed.
    pub fn restore(
        entries: impl IntoIterator<Item = (RepoPathBuf, OverlayEntry)>,
        synced: u64,
    ) -> Self {
        let entries: BTreeMap<_, _> = entries.into_iter().collect();
        let next_seq = entries
            .values()
            .map(|e| e.seq)
            .max()
            .unwrap_or(0)
            .max(synced)
            + 1;
        Overlay {
            entries,
            next_seq,
            synced,
        }
    }

    /// The entry for `path`.
    pub fn get(&self, path: &RepoPath) -> Option<&OverlayEntry> {
        self.entries.get(path)
    }

    /// Every entry, in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&RepoPathBuf, &OverlayEntry)> {
        self.entries.iter()
    }

    /// Entries at or below `dir` (every entry for the root).
    pub fn under<'a>(
        &'a self,
        dir: &'a RepoPath,
    ) -> impl Iterator<Item = (&'a RepoPathBuf, &'a OverlayEntry)> + 'a {
        let start = if dir.is_root() {
            Bound::Unbounded
        } else {
            Bound::Included(dir.to_owned())
        };
        // Paths below `dir` sort in [`dir/`, `dir0`), but siblings such as
        // `dir-x` sort among them, so the range is filtered too.
        let end = format!("{}0", dir.as_str());
        self.entries
            .range((start, Bound::Unbounded))
            .take_while(move |(p, _)| dir.is_root() || p.as_str() < end.as_str())
            .filter(move |(p, _)| p.starts_with(dir))
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True with no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry recording `op` on `path` would become; apply it with
    /// [`Overlay::apply`] once it is persisted.
    pub fn next(&self, op: OverlayOp, conflict: bool) -> OverlayEntry {
        OverlayEntry {
            op,
            seq: self.next_seq,
            conflict,
        }
    }

    /// Apply a persisted entry, returning the one it replaces.
    pub fn apply(&mut self, path: RepoPathBuf, entry: OverlayEntry) -> Option<OverlayEntry> {
        self.next_seq = self.next_seq.max(entry.seq + 1);
        self.entries.insert(path, entry)
    }

    /// Reserve the next sequence number (several entries in one batch).
    pub fn reserve(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    /// Forget a path's entry (it matches the base again and is synced).
    pub fn remove(&mut self, path: &RepoPath) -> Option<OverlayEntry> {
        self.entries.remove(path)
    }

    /// Everything recorded up to this sequence number has been pushed.
    pub fn synced(&self) -> u64 {
        self.synced
    }

    /// The highest sequence number recorded.
    pub fn last_seq(&self) -> u64 {
        self.next_seq - 1
    }

    /// Entries not yet pushed.
    pub fn pending(&self) -> impl Iterator<Item = (&RepoPathBuf, &OverlayEntry)> {
        let synced = self.synced;
        self.entries.iter().filter(move |(_, e)| e.seq > synced)
    }

    /// Count of entries not yet pushed.
    pub fn pending_count(&self) -> usize {
        self.pending().count()
    }

    /// Record a push of everything up to `seq`.
    pub fn mark_synced(&mut self, seq: u64) {
        self.synced = self.synced.max(seq);
    }

    /// Forget every pushed entry.
    pub fn mark_unsynced(&mut self) {
        self.synced = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{FileKind, ObjectHash, ObjectKind};

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    fn set(n: u8) -> OverlayOp {
        OverlayOp::Set {
            digest: ObjectHash::digest(ObjectKind::Blob, &[n]),
            kind: FileKind::Regular,
            size: 1,
        }
    }

    #[test]
    fn later_writes_replace_earlier_ones_and_only_new_ones_are_pending() {
        let mut o = Overlay::restore([], 0);
        let e = o.next(set(1), false);
        o.apply(p("a"), e);
        let e = o.next(set(2), false);
        o.apply(p("b"), e);
        o.mark_synced(o.last_seq());
        let e = o.next(OverlayOp::Delete, false);
        o.apply(p("a"), e);
        let pending: Vec<_> = o.pending().map(|(p, _)| p.as_str().to_string()).collect();
        assert_eq!(pending, ["a"]);
        assert_eq!(o.get(&p("a")).unwrap().op, OverlayOp::Delete);
    }

    #[test]
    fn under_lists_a_directory_and_not_its_siblings() {
        let mut o = Overlay::restore([], 0);
        for path in ["a-b", "a/x", "a/y/z", "ab", "b"] {
            let e = o.next(set(0), false);
            o.apply(p(path), e);
        }
        let a = p("a");
        let under: Vec<_> = o.under(&a).map(|(p, _)| p.as_str()).collect();
        assert_eq!(under, ["a/x", "a/y/z"]);
        assert_eq!(o.under(&RepoPathBuf::root()).count(), 5);
    }

    #[test]
    fn restored_sequences_continue_after_the_highest() {
        let entry = OverlayEntry {
            op: set(1),
            seq: 7,
            conflict: false,
        };
        let o = Overlay::restore([(p("a"), entry)], 3);
        assert_eq!(o.next(set(2), false).seq, 8);
        assert_eq!(o.pending_count(), 1);
    }
}

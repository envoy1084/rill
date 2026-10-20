//! Where a large directory's nodes begin and end.
//!
//! Whether a node ends after an item depends only on the item's key and
//! the level, plus a size cap counted from the node's start. Chunking a
//! sorted sequence left to right is therefore a function of the sequence
//! alone: equal directories get equal nodes however they were edited, and
//! an edit changes nodes only near it.

/// Directories with more entries than this are split into nodes.
pub const SHARD_ENTRIES: usize = 4096;
/// Expected items per leaf.
pub const LEAF_TARGET: u64 = 1024;
/// Most items in one leaf.
pub const MAX_LEAF: usize = 4096;
/// Expected children per inner node.
pub const INNER_TARGET: u64 = 64;
/// Most children in one inner node.
pub const MAX_INNER: usize = 256;

/// Whether a node at `level` (0 for leaves) ends after the item `key`.
pub fn ends_after(level: u8, key: &str) -> bool {
    let target = if level == 0 {
        LEAF_TARGET
    } else {
        INNER_TARGET
    };
    let mut hasher = blake3::Hasher::new_derive_key("rill tree node boundary");
    hasher.update(&[level]);
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    let word: [u8; 8] = digest.as_bytes()[..8]
        .try_into()
        .expect("a digest has 32 bytes");
    u64::from_le_bytes(word) % target == 0
}

/// Most items in one node at `level`.
pub fn max_items(level: u8) -> usize {
    if level == 0 {
        MAX_LEAF
    } else {
        MAX_INNER
    }
}

/// Splits a sorted sequence into nodes as items arrive.
pub struct Chunker<T> {
    level: u8,
    current: Vec<T>,
}

impl<T> Chunker<T> {
    pub fn new(level: u8) -> Self {
        Chunker {
            level,
            current: Vec::new(),
        }
    }

    /// Add the next item; returns a finished node when this item ends one.
    pub fn push(&mut self, item: T, key: &str) -> Option<Vec<T>> {
        let ends = ends_after(self.level, key);
        self.current.push(item);
        (ends || self.current.len() >= max_items(self.level))
            .then(|| std::mem::take(&mut self.current))
    }

    /// True when the last push ended a node (or nothing was pushed).
    pub fn at_boundary(&self) -> bool {
        self.current.is_empty()
    }

    /// The unfinished last node, if any.
    pub fn finish(self) -> Option<Vec<T>> {
        (!self.current.is_empty()).then_some(self.current)
    }
}

/// Split all of `items` into nodes at `level`.
pub fn chunk<T>(items: Vec<T>, level: u8, key: impl Fn(&T) -> &str) -> Vec<Vec<T>> {
    let mut chunker = Chunker::new(level);
    let mut out = Vec::new();
    for item in items {
        let k = key(&item).to_owned();
        out.extend(chunker.push(item, &k));
    }
    out.extend(chunker.finish());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("file-{i:07}.rs")).collect()
    }

    #[test]
    fn nodes_are_bounded_and_cover_everything() {
        let items = names(50_000);
        let nodes = chunk(items.clone(), 0, |s| s.as_str());
        assert!(nodes.iter().all(|n| !n.is_empty() && n.len() <= MAX_LEAF));
        assert_eq!(nodes.concat(), items);
        let mean = items.len() / nodes.len();
        assert!((256..=4096).contains(&mean), "mean leaf size {mean}");
    }

    #[test]
    fn an_insert_changes_only_nearby_nodes() {
        let items = names(50_000);
        let before = chunk(items.clone(), 0, |s| s.as_str());
        let mut edited = items;
        edited.insert(25_000, "file-0025000a.rs".into());
        let after = chunk(edited, 0, |s| s.as_str());
        let changed = after.iter().filter(|n| !before.contains(n)).count();
        assert!(changed <= 2, "{changed} nodes changed");
    }
}

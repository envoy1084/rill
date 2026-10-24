//! A root's files as queries see them: a base map plus the overlay of
//! every delta above it.

use std::collections::BTreeMap;
use std::sync::Arc;

use rill_types::{FileKind, ObjectHash, RepoPath, RepoPathBuf};

use super::hasher::ContentMap;
use super::layer::{PathChange, PathEntry};

/// A base map and the index from content to the paths holding it.
#[derive(Debug, Default)]
pub struct BaseMap {
    entries: Vec<PathEntry>,
    /// Each content's first entry; a map of lists cost a gigabyte at ten
    /// million files, and most contents sit at one path.
    first: ContentMap<u32>,
    /// The further entries of contents at several paths.
    more: ContentMap<Vec<u32>>,
}

impl BaseMap {
    pub fn new(mut entries: Vec<PathEntry>) -> BaseMap {
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        let mut first = ContentMap::default();
        first.reserve(entries.len());
        let mut more: ContentMap<Vec<u32>> = ContentMap::default();
        for (i, e) in entries.iter().enumerate() {
            if first.try_insert_first(e.hash, i as u32) {
                continue;
            }
            more.entry(e.hash).or_default().push(i as u32);
        }
        BaseMap {
            entries,
            first,
            more,
        }
    }

    /// The entries holding `hash`.
    fn holding(&self, hash: &ObjectHash) -> impl Iterator<Item = &PathEntry> {
        self.first
            .get(hash)
            .into_iter()
            .chain(self.more.get(hash).into_iter().flatten())
            .map(|&i| &self.entries[i as usize])
    }
}

trait InsertFirst {
    /// Insert unless the key is present; whether it was inserted.
    fn try_insert_first(&mut self, hash: ObjectHash, entry: u32) -> bool;
}

impl InsertFirst for ContentMap<u32> {
    fn try_insert_first(&mut self, hash: ObjectHash, entry: u32) -> bool {
        match self.entry(hash) {
            std::collections::hash_map::Entry::Occupied(_) => false,
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(entry);
                true
            }
        }
    }
}

/// The files of one root.
#[derive(Debug, Clone)]
pub struct PathView {
    base: Arc<BaseMap>,
    /// Paths the deltas set (`Some`) or removed (`None`).
    overlay: Arc<BTreeMap<RepoPathBuf, Option<(ObjectHash, FileKind)>>>,
    /// The overlay's paths by content.
    overlay_by_hash: Arc<ContentMap<Vec<RepoPathBuf>>>,
    /// Base contents with a path the overlay replaces: only their paths
    /// need checking against the overlay.
    shadowed: Arc<ContentMap<()>>,
    /// How many deltas lie between this root and its base; 0 for a base.
    pub depth: u32,
}

impl PathView {
    /// The view of a base root.
    pub fn base(entries: Vec<PathEntry>) -> PathView {
        PathView {
            base: Arc::new(BaseMap::new(entries)),
            overlay: Arc::default(),
            overlay_by_hash: Arc::default(),
            shadowed: Arc::default(),
            depth: 0,
        }
    }

    /// This view with a delta's `changes` applied on top.
    pub fn apply(&self, changes: &[PathChange]) -> PathView {
        let mut overlay = (*self.overlay).clone();
        for (path, state) in changes {
            overlay.insert(path.clone(), *state);
        }
        let mut by_hash: ContentMap<Vec<RepoPathBuf>> = ContentMap::default();
        let mut shadowed: ContentMap<()> = ContentMap::default();
        for (path, state) in &overlay {
            if let Some((hash, _)) = state {
                by_hash.entry(*hash).or_default().push(path.clone());
            }
            if let Ok(i) = self
                .base
                .entries
                .binary_search_by(|e| e.path.as_path().cmp(path))
            {
                shadowed.insert(self.base.entries[i].hash, ());
            }
        }
        PathView {
            base: self.base.clone(),
            overlay: Arc::new(overlay),
            overlay_by_hash: Arc::new(by_hash),
            shadowed: Arc::new(shadowed),
            depth: self.depth + 1,
        }
    }

    /// The file at `path`.
    pub fn get(&self, path: &RepoPath) -> Option<(ObjectHash, FileKind)> {
        if let Some(state) = self.overlay.get(path) {
            return *state;
        }
        self.base
            .entries
            .binary_search_by(|e| e.path.as_path().cmp(path))
            .ok()
            .map(|i| (self.base.entries[i].hash, self.base.entries[i].kind))
    }

    /// Every path holding the content `hash`, in path order.
    pub fn paths_of(&self, hash: &ObjectHash) -> Vec<RepoPathBuf> {
        let mut out: Vec<RepoPathBuf> = self.paths(hash).cloned().collect();
        out.sort();
        out
    }

    /// Whether any path holds `hash`.
    /// The paths holding `hash`, borrowed and unordered.
    pub fn paths<'a>(&'a self, hash: &ObjectHash) -> impl Iterator<Item = &'a RepoPathBuf> + 'a {
        let extra = self.overlay_by_hash.get(hash).into_iter().flatten();
        let check = self.shadowed.contains_key(hash);
        self.base
            .holding(hash)
            .map(|e| &e.path)
            .filter(move |p| !check || !self.overlay.contains_key(*p))
            .chain(extra)
    }

    pub fn holds(&self, hash: &ObjectHash) -> bool {
        self.paths(hash).next().is_some()
    }

    /// Every file, in path order.
    pub fn entries(&self) -> Vec<PathEntry> {
        let mut out: Vec<PathEntry> = self
            .base
            .entries
            .iter()
            .filter(|e| !self.overlay.contains_key(&e.path))
            .cloned()
            .collect();
        out.extend(self.overlay.iter().filter_map(|(path, state)| {
            state.map(|(hash, kind)| PathEntry {
                path: path.clone(),
                hash,
                kind,
            })
        }));
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// Number of files.
    pub fn len(&self) -> usize {
        let removed = self
            .overlay
            .keys()
            .filter(|p| {
                self.base
                    .entries
                    .binary_search_by(|e| e.path.as_path().cmp(p))
                    .is_ok()
            })
            .count();
        let added = self.overlay.values().filter(|s| s.is_some()).count();
        self.base.entries.len() - removed + added
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Paths this view's deltas changed: recently modified files, for
    /// ranking.
    pub fn recently_changed(&self, path: &RepoPath) -> bool {
        self.overlay.contains_key(path)
    }
}

#[cfg(test)]
mod tests {
    use rill_types::ObjectKind;

    use super::*;

    fn h(s: &str) -> ObjectHash {
        ObjectHash::digest(ObjectKind::Blob, s.as_bytes())
    }

    fn entry(p: &str, c: &str) -> PathEntry {
        PathEntry {
            path: RepoPathBuf::new(p).unwrap(),
            hash: h(c),
            kind: FileKind::Regular,
        }
    }

    #[test]
    fn deltas_override_the_base_and_track_content() {
        let base = PathView::base(vec![entry("a", "x"), entry("b", "x"), entry("c", "y")]);
        let p = |s: &str| RepoPathBuf::new(s).unwrap();
        let next = base.apply(&[
            (p("b"), None),
            (p("d"), Some((h("x"), FileKind::Regular))),
            (p("c"), Some((h("z"), FileKind::Regular))),
        ]);
        assert_eq!(next.paths_of(&h("x")), [p("a"), p("d")]);
        assert!(!next.holds(&h("y")), "c changed away from y");
        assert!(base.holds(&h("y")), "the base view is untouched");
        assert_eq!(next.len(), 3);
        assert_eq!(next.get(p("c").as_path()).unwrap().0, h("z"));
        let paths: Vec<_> = next.entries().into_iter().map(|e| e.path).collect();
        assert_eq!(paths, [p("a"), p("c"), p("d")]);
        assert_eq!(next.apply(&[]).depth, 2);
    }
}

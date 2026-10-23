//! What a path is in the view: the overlay over the base, plus the
//! files the view synthesizes (the checkout marker) and the directories
//! made locally. Listings and lookups read
//! trees only; no file content is fetched.

use std::collections::BTreeMap;

use bytes::Bytes;
use rill_types::{ContentRef, EntryKind, FileKind, ObjectHash, RepoPath};

use super::view::View;
use crate::cache::local_digest;
use crate::error::Result;
use crate::overlay::OverlayOp;

/// Where a file's bytes come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The base tree.
    Base(ContentRef),
    /// The overlay (local digest).
    Local(ObjectHash),
    /// A read-only file the view synthesizes.
    Virtual(Bytes),
}

impl Source {
    /// The hash mtimes derive from.
    pub fn hash(&self) -> ObjectHash {
        match self {
            Source::Base(c) => c.content_hash(),
            Source::Local(d) => *d,
            Source::Virtual(b) => local_digest(b),
        }
    }
}

/// A path in the view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A directory.
    Dir,
    /// A file.
    File {
        size: u64,
        kind: FileKind,
        source: Source,
    },
}

impl Node {
    pub fn is_dir(&self) -> bool {
        matches!(self, Node::Dir)
    }
}

impl View {
    /// What `path` is, if it exists.
    pub(crate) async fn resolve(&self, path: &RepoPath) -> Result<Option<Node>> {
        if path.is_root() {
            return Ok(Some(Node::Dir));
        }
        if let Some(bytes) = self.virtuals.get(path) {
            return Ok(Some(Node::File {
                size: bytes.len() as u64,
                kind: FileKind::Regular,
                source: Source::Virtual(bytes.clone()),
            }));
        }
        if self.virtuals.keys().any(|v| v.starts_with(path)) {
            return Ok(Some(Node::Dir));
        }
        match self.mount.entry(path).map(|e| e.op) {
            Some(OverlayOp::Set { digest, kind, size }) => {
                return Ok(Some(Node::File {
                    size,
                    kind,
                    source: Source::Local(digest),
                }))
            }
            Some(OverlayOp::Delete) => return Ok(self.local_dir(path).then_some(Node::Dir)),
            None => {}
        }
        match self.mount.base().entry(path).await? {
            Some(EntryKind::File { content, kind }) => Ok(Some(Node::File {
                size: content.size(),
                kind,
                source: Source::Base(content),
            })),
            Some(EntryKind::Dir { .. }) if !self.dirs.lock().removed.contains(path) => {
                Ok(Some(Node::Dir))
            }
            _ => Ok(self.local_dir(path).then_some(Node::Dir)),
        }
    }

    /// A directory made in the view, or one that holds a file added in it.
    fn local_dir(&self, path: &RepoPath) -> bool {
        if self.dirs.lock().created.contains(path) {
            return true;
        }
        self.mount
            .entries_under(path)
            .iter()
            .any(|(p, e)| p.as_path() != path && matches!(e.op, OverlayOp::Set { .. }))
    }

    /// The entries of directory `dir` (names and whether each is a
    /// directory), or `None` when it is not one.
    pub(crate) async fn list(&self, dir: &RepoPath) -> Result<Option<BTreeMap<String, bool>>> {
        if !self.resolve(dir).await?.is_some_and(|n| n.is_dir()) {
            return Ok(None);
        }
        let mut names = BTreeMap::new();
        let removed = self.dirs.lock().removed.clone();
        if let Some(tree) = self.mount.base().dir(dir).await? {
            for (name, entry) in &tree.entries {
                let child = dir.to_owned_path().join(name)?;
                let is_dir = matches!(entry.kind, EntryKind::Dir { .. });
                if is_dir && removed.contains(&child) {
                    continue;
                }
                names.insert(name.clone(), is_dir);
            }
        }
        for (path, entry) in self.mount.entries_under(dir) {
            let Some(rest) = path.strip_prefix(dir) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(first) = parts.next() else {
                continue;
            };
            let deeper = parts.next().is_some();
            match (entry.op, deeper) {
                (OverlayOp::Set { .. }, false) => {
                    names.insert(first.to_string(), false);
                }
                (OverlayOp::Set { .. }, true) => {
                    names.insert(first.to_string(), true);
                }
                (OverlayOp::Delete, false) => {
                    if names.get(first) == Some(&false) {
                        names.remove(first);
                    }
                }
                (OverlayOp::Delete, true) => {}
            }
        }
        for created in &self.dirs.lock().created {
            if created.parent() == dir && !created.is_root() {
                if let Some(name) = created.file_name() {
                    names.insert(name.to_string(), true);
                }
            }
        }
        for virtual_path in self.virtuals.keys() {
            let Some(rest) = virtual_path.strip_prefix(dir) else {
                continue;
            };
            let mut parts = rest.components();
            if let Some(first) = parts.next() {
                names.insert(first.to_string(), parts.next().is_some());
            }
        }
        Ok(Some(names))
    }
}

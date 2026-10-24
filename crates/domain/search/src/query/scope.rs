//! What a query searches: an indexed root's files, plus files read live
//! (a workspace's edits, or a version's changes since the nearest indexed
//! root) that take their paths' place.

use std::sync::Arc;

use rill_types::{ContentRef, RepoPathBuf};

use crate::paths::PathView;

/// A file searched by reading it, not from the index.
#[derive(Debug, Clone)]
pub struct LiveFile {
    pub path: RepoPathBuf,
    pub content: ContentRef,
}

/// The files one query covers.
#[derive(Debug, Clone)]
pub struct Scope {
    /// The indexed files. Paths the live files replace or remove must
    /// already be applied to it.
    pub view: Arc<PathView>,
    pub live: Vec<LiveFile>,
}

impl Scope {
    /// Exactly an indexed root.
    pub fn indexed(view: Arc<PathView>) -> Scope {
        Scope {
            view,
            live: Vec::new(),
        }
    }
}

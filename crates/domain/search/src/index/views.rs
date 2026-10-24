//! Building a root's [`PathView`] from its layer chain, with a cache.

use std::sync::Arc;

use dashmap::DashMap;
use rill_store::RepoStore;
use rill_types::TreeHash;

use super::records;
use crate::error::{Error, Result};
use crate::paths::{PathLayer, PathView};

/// Views cached per process.
const MAX_VIEWS: usize = 64;

#[derive(Default)]
pub struct ViewCache {
    views: DashMap<TreeHash, Arc<PathView>>,
}

impl ViewCache {
    /// The view of `root`, or `None` when the root is not indexed.
    pub async fn get(&self, repo: &RepoStore, root: TreeHash) -> Result<Option<Arc<PathView>>> {
        if let Some(v) = self.views.get(&root) {
            return Ok(Some(v.clone()));
        }
        // Walk down to a cached view or a base, then apply deltas upward.
        let mut chain = Vec::new();
        let mut at = root;
        let mut view = loop {
            if let Some(v) = self.views.get(&at) {
                break v.clone();
            }
            match records::layer(repo, &at).await? {
                None if chain.is_empty() => return Ok(None),
                None => {
                    return Err(Error::Codec(format!(
                        "path layer of {at} is missing below {root}"
                    )))
                }
                Some(PathLayer::Base(entries)) => break Arc::new(PathView::base(entries)),
                Some(PathLayer::Delta {
                    parent, changes, ..
                }) => {
                    chain.push((at, changes));
                    at = parent;
                }
            }
        };
        self.views.insert(at, view.clone());
        for (r, changes) in chain.into_iter().rev() {
            view = Arc::new(view.apply(&changes));
            self.views.insert(r, view.clone());
        }
        if self.views.len() > MAX_VIEWS {
            let keep = root;
            self.views.retain(|r, _| *r == keep);
            self.views.insert(root, view.clone());
        }
        Ok(Some(view))
    }

    pub fn insert(&self, root: TreeHash, view: Arc<PathView>) {
        self.views.insert(root, view);
    }
}

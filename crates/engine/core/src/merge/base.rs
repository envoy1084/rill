//! The base of a change-level merge, including criss-cross histories.
//!
//! With one best common ancestor the base is that version. With several,
//! the base is *virtual*: per path, the ancestors are merged pairwise
//! (recursively, each pair against its own bases), like git's recursive
//! strategy. A path whose virtual merge conflicts keeps the conflict terms;
//! the algebra flattens them into the outer merge.

use std::collections::BTreeSet;
use std::sync::Arc;

use futures::future::BoxFuture;
use rill_types::objects::change::FileState;
use rill_types::{ChangeVersionHash, Merge, RepoPath, TreeHash};

use super::content::Structural;
use super::state::{change_terms, merge_file_state_with};
use crate::derive::{DeriveManager, RootTree};
use crate::diff::file_state;
use crate::error::Result;
use crate::graph::CommitGraph;

/// A merge base: a stored version or a virtual merge of several.
#[derive(Debug, Clone)]
pub(crate) enum MergeBase {
    Version(TreeHash),
    Virtual {
        base: Option<Box<MergeBase>>,
        left: Box<MergeBase>,
        right: Box<MergeBase>,
    },
}

impl MergeBase {
    /// The base for merging `a` and `b`, or `None` when they share no
    /// history.
    pub(crate) async fn of(
        derive: &Arc<DeriveManager>,
        graph: &CommitGraph,
        a: &ChangeVersionHash,
        b: &ChangeVersionHash,
    ) -> Result<Option<MergeBase>> {
        let bases = graph.merge_bases(a, b).await?;
        Self::from_bases(derive, graph, bases).await
    }

    fn from_bases<'a>(
        derive: &'a Arc<DeriveManager>,
        graph: &'a CommitGraph,
        bases: Vec<ChangeVersionHash>,
    ) -> BoxFuture<'a, Result<Option<MergeBase>>> {
        Box::pin(async move {
            let Some((first, rest)) = bases.split_first() else {
                return Ok(None);
            };
            let mut acc = MergeBase::Version(derive.get::<RootTree>(first).await?);
            for other in rest {
                let shared = graph.merge_bases(first, other).await?;
                acc = MergeBase::Virtual {
                    base: Self::from_bases(derive, graph, shared).await?.map(Box::new),
                    left: Box::new(acc),
                    right: Box::new(MergeBase::Version(derive.get::<RootTree>(other).await?)),
                };
            }
            Ok(Some(acc))
        })
    }

    /// Root trees of every stored version the base is built from.
    pub(crate) fn roots(&self) -> BTreeSet<TreeHash> {
        let mut out = BTreeSet::new();
        self.collect_roots(&mut out);
        out
    }

    fn collect_roots(&self, out: &mut BTreeSet<TreeHash>) {
        match self {
            MergeBase::Version(root) => {
                out.insert(*root);
            }
            MergeBase::Virtual { base, left, right } => {
                if let Some(base) = base {
                    base.collect_roots(out);
                }
                left.collect_roots(out);
                right.collect_roots(out);
            }
        }
    }

    /// The base's state at `path`: resolved for a stored version and for a
    /// virtual base that merges cleanly, the conflict terms otherwise.
    pub(crate) fn terms<'a>(
        &'a self,
        derive: &'a Arc<DeriveManager>,
        path: &'a RepoPath,
    ) -> BoxFuture<'a, Result<Merge<FileState>>> {
        Box::pin(async move {
            match self {
                MergeBase::Version(root) => Ok(Merge::resolved(
                    file_state(derive.trees(), Some(*root), path).await?,
                )),
                MergeBase::Virtual { base, left, right } => {
                    let base = match base {
                        Some(base) => base.terms(derive, path).await?,
                        None => Merge::resolved(None),
                    };
                    let left = left.terms(derive, path).await?;
                    let right = right.terms(derive, path).await?;
                    let terms = Merge::three_way(base, left, right).flatten();
                    let objects = derive.repo().objects();
                    let structural = derive.merger().map(|merger| Structural { merger, path });
                    let merged =
                        merge_file_state_with(terms, objects, objects.as_ref(), structural)
                            .await?
                            .change;
                    Ok(match merged.resolved_state() {
                        Some(state) => Merge::resolved(state),
                        None => change_terms(&merged),
                    })
                }
            }
        })
    }
}

//! The root tree of a version, materialized from its first parent's.

use async_trait::async_trait;
use rill_types::{ChangeVersion, TreeHash};

use super::ctx::DeriveCtx;
use super::traits::Derivable;
use crate::error::Result;

/// Root tree hash of a change version.
pub struct RootTree;

#[async_trait]
impl Derivable for RootTree {
    const KIND: &'static str = "root";
    const VERSION: u8 = 1;
    type Value = TreeHash;

    async fn derive(
        ctx: &DeriveCtx,
        cv: &ChangeVersion,
        parents: Vec<TreeHash>,
    ) -> Result<TreeHash> {
        // File changes are relative to the first parent; other parents'
        // trees are irrelevant here.
        let parent_root = parents.first().copied();
        ctx.trees().materialize(parent_root, cv, ctx).await
    }
}

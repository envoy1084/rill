//! Generation numbers: `1 + max(parent generations)`, 1 for roots.

use async_trait::async_trait;
use rill_types::ChangeVersion;

use super::ctx::DeriveCtx;
use super::traits::Derivable;
use crate::error::Result;

/// Distance from the furthest root, used to prune ancestry walks.
pub struct Generation;

#[async_trait]
impl Derivable for Generation {
    const KIND: &'static str = "gen";
    const VERSION: u8 = 1;
    type Value = u64;

    async fn derive(_ctx: &DeriveCtx, _cv: &ChangeVersion, parents: Vec<u64>) -> Result<u64> {
        Ok(1 + parents.into_iter().max().unwrap_or(0))
    }
}

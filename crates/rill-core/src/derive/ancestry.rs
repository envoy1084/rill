//! Generation numbers and skip pointers for fast ancestry queries.
//!
//! Every version records its first-parent depth and one jump pointer to a
//! first-parent ancestor, chosen by the skew-binary rule (Myers 1983): a
//! version jumps two of its parent's jumps when those two cover equal
//! distances, and to its parent otherwise. Any first-parent ancestor at a
//! given depth is then reachable in O(log depth) steps. Histories without
//! merges are trees, so those steps answer ancestry exactly; with a merge
//! anywhere behind a version, callers fall back to a full walk.

use async_trait::async_trait;
use rill_types::{ChangeVersion, ChangeVersionHash, Encodable};
use serde::{Deserialize, Serialize};

use super::ctx::DeriveCtx;
use super::traits::{Derivable, Schedule};
use crate::error::Result;

/// Where a version sits in the history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AncestryInfo {
    /// `1 + max(parent generations)`; 1 for a root.
    pub generation: u64,
    /// First-parent distance from a root; 0 for a root.
    pub depth: u64,
    /// Whether the version or any ancestor has more than one parent.
    pub merges: bool,
    pub parent: Option<ChangeVersionHash>,
    /// A first-parent ancestor (the version itself for a root).
    pub jump: ChangeVersionHash,
    pub jump_depth: u64,
}

/// [`AncestryInfo`] of every version.
pub struct Ancestry;

#[async_trait]
impl Derivable for Ancestry {
    const KIND: &'static str = "anc";
    const VERSION: u8 = 1;
    const SCHEDULE: Schedule = Schedule::LandCritical;
    type Value = AncestryInfo;

    async fn derive(
        ctx: &DeriveCtx,
        cv: &ChangeVersion,
        parents: Vec<AncestryInfo>,
    ) -> Result<AncestryInfo> {
        let generation = 1 + parents.iter().map(|p| p.generation).max().unwrap_or(0);
        let merges = cv.parents.len() > 1 || parents.iter().any(|p| p.merges);
        let (Some(&parent), Some(first)) = (cv.parents.first(), parents.first()) else {
            return Ok(AncestryInfo {
                generation,
                depth: 0,
                merges,
                parent: None,
                jump: cv.hash(),
                jump_depth: 0,
            });
        };
        let jump_of_jump = ctx.manager().get::<Ancestry>(&first.jump).await?;
        let (jump, jump_depth) =
            if first.depth - first.jump_depth == first.jump_depth - jump_of_jump.jump_depth {
                (jump_of_jump.jump, jump_of_jump.jump_depth)
            } else {
                (parent, first.depth)
            };
        Ok(AncestryInfo {
            generation,
            depth: first.depth + 1,
            merges,
            parent: Some(parent),
            jump,
            jump_depth,
        })
    }
}

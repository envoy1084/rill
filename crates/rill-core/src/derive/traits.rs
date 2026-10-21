//! The [`Derivable`] trait.

use async_trait::async_trait;
use rill_types::ChangeVersion;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::ctx::DeriveCtx;
use crate::error::Result;

/// When a derived value is computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schedule {
    /// Before a stored version is acknowledged and before it lands:
    /// landing reads it.
    LandCritical,
    /// By the background worker for every trunk version, ahead of reads.
    Background,
    /// When first read.
    OnDemand,
}

/// A value computable from a change version and the same value of its
/// parents.
///
/// Implementations are pure functions of the version's content and its
/// ancestors, so a derived value never needs invalidation: a new version
/// is a new key. Bump `VERSION` when the computation changes; old entries
/// are simply never read again.
#[async_trait]
pub trait Derivable: Send + Sync + 'static {
    /// Short stable name used in the storage key.
    const KIND: &'static str;
    /// Version of the computation, part of the storage key.
    const VERSION: u8;
    /// Whether `derive` needs the parents' values. When false the manager
    /// skips computing them and passes an empty vector.
    const NEEDS_PARENTS: bool = true;
    /// When the value is computed.
    const SCHEDULE: Schedule = Schedule::OnDemand;
    /// The derived value.
    type Value: Serialize + DeserializeOwned + Send + Sync + Clone + 'static;

    /// Compute the value for `cv`; `parents[i]` is the value for
    /// `cv.parents[i]` when `NEEDS_PARENTS`.
    async fn derive(
        ctx: &DeriveCtx,
        cv: &ChangeVersion,
        parents: Vec<Self::Value>,
    ) -> Result<Self::Value>;
}

/// Storage key of a derivable: `kind@version`.
pub fn derived_kind_key<D: Derivable + ?Sized>() -> String {
    format!("{}@{}", D::KIND, D::VERSION)
}

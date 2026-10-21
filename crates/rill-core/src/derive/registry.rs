//! The built-in derived kinds and when each is computed.

use std::sync::Arc;

use futures::future::BoxFuture;
use rill_types::ChangeVersionHash;

use super::manager::DeriveManager;
use super::traits::{derived_kind_key, Derivable, Schedule};
use super::{Ancestry, ChangeSummary, ChangedPaths, PathHistory, RootTree};
use crate::error::Result;

/// One derived kind computed per version.
pub struct KindSpec {
    /// Storage key, `kind@version`.
    pub key: String,
    pub schedule: Schedule,
    compute: fn(Arc<DeriveManager>, ChangeVersionHash) -> BoxFuture<'static, Result<()>>,
    stored: fn(Arc<DeriveManager>, ChangeVersionHash) -> BoxFuture<'static, Result<bool>>,
}

impl KindSpec {
    fn of<D: Derivable>() -> Self {
        KindSpec {
            key: derived_kind_key::<D>(),
            schedule: D::SCHEDULE,
            compute: |m, h| Box::pin(async move { m.get::<D>(&h).await.map(|_| ()) }),
            stored: |m, h| Box::pin(async move { m.has::<D>(&h).await }),
        }
    }

    /// Compute (or load) the value for `hash`.
    pub async fn compute(
        &self,
        manager: &Arc<DeriveManager>,
        hash: ChangeVersionHash,
    ) -> Result<()> {
        (self.compute)(manager.clone(), hash).await
    }

    /// Whether the value for `hash` is stored.
    pub async fn is_stored(
        &self,
        manager: &Arc<DeriveManager>,
        hash: ChangeVersionHash,
    ) -> Result<bool> {
        (self.stored)(manager.clone(), hash).await
    }
}

/// Every built-in kind that is keyed by version. Blame is keyed by path
/// as well and is always computed on demand.
pub fn kinds() -> Vec<KindSpec> {
    vec![
        KindSpec::of::<RootTree>(),
        KindSpec::of::<ChangedPaths>(),
        KindSpec::of::<Ancestry>(),
        KindSpec::of::<PathHistory>(),
        KindSpec::of::<ChangeSummary>(),
    ]
}

/// The kinds with `schedule`.
pub fn scheduled(schedule: Schedule) -> Vec<KindSpec> {
    kinds()
        .into_iter()
        .filter(|k| k.schedule == schedule)
        .collect()
}

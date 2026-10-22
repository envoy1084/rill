//! Each request's footprint, for placing it among the others: the paths
//! it changed and, when the repository declares a build graph, the
//! targets it touches.

use std::collections::BTreeSet;
use std::sync::Arc;

use rill_core::{ChangedPaths, RootTree};
use rill_types::ChangeVersionHash;

use super::lander::Lander;
use crate::error::Result;
use crate::queue::QueuedRequest;
use crate::schedule::{Footprint, LandingRules};

/// Versions whose touched targets are remembered; past it the memory is
/// cleared.
const TOUCHED_CACHE: usize = 4096;

/// What [`Lander::footprint`] remembers of each version's targets.
pub(super) type TouchedCache =
    std::collections::HashMap<ChangeVersionHash, Option<Arc<BTreeSet<String>>>>;

impl Lander {
    /// What `queued`'s version changed, under `rules`.
    pub(super) async fn footprint(
        &self,
        queued: &QueuedRequest,
        rules: &LandingRules,
    ) -> Result<Footprint> {
        let version = queued.request.version;
        let paths = self.derive.get::<ChangedPaths>(&version).await?;
        let footprint = Footprint::new(paths, rules);
        Ok(match self.touched_targets(&version).await? {
            Some(targets) => footprint.with_targets(targets.as_ref().clone()),
            None => footprint,
        })
    }

    /// The build targets `version` touches against its first parent,
    /// asked once per version.
    async fn touched_targets(
        &self,
        version: &ChangeVersionHash,
    ) -> Result<Option<Arc<BTreeSet<String>>>> {
        let Some(graph) = &self.targets else {
            return Ok(None);
        };
        if let Some(known) = self.touched.lock().get(version) {
            return Ok(known.clone());
        }
        let cv = self.derive.change(version).await?;
        let root = self.derive.get::<RootTree>(version).await?;
        let base = match cv.first_parent() {
            Some(parent) => Some(self.derive.get::<RootTree>(&parent).await?),
            None => None,
        };
        let touched = graph.touched(root, base).await?.map(Arc::new);
        let mut cache = self.touched.lock();
        if cache.len() >= TOUCHED_CACHE {
            cache.clear();
        }
        cache.insert(*version, touched.clone());
        Ok(touched)
    }
}

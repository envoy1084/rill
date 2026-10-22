//! Reading what the sequencer needs to place requests: the landing rules
//! on trunk and each request's footprint.

use std::sync::Arc;

use rill_core::{ChangedPaths, RootTree};
use rill_types::{ChangeVersionHash, EntryKind, RepoPathBuf};

use super::lander::Lander;
use crate::error::Result;
use crate::queue::QueuedRequest;
use crate::schedule::{Footprint, LandingRules, RULES_PATH};

impl Lander {
    /// The landing rules at trunk `head`, read once per head.
    pub(super) async fn landing_rules(
        &self,
        head: Option<ChangeVersionHash>,
    ) -> Result<Arc<LandingRules>> {
        if let Some((at, rules)) = self.rules.lock().as_ref() {
            if *at == head {
                return Ok(rules.clone());
            }
        }
        let rules = match head {
            None => LandingRules::default(),
            Some(head) => {
                let root = self.derive.get::<RootTree>(&head).await?;
                let path = RepoPathBuf::new(RULES_PATH)?;
                match self.derive.trees().lookup(root, &path).await? {
                    Some(EntryKind::File { content, .. }) => {
                        let bytes =
                            rill_core::read_content(self.repo.objects().as_ref(), &content).await?;
                        LandingRules::parse(&String::from_utf8_lossy(&bytes)).unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "unreadable landing rules; using the defaults");
                            LandingRules::default()
                        })
                    }
                    _ => LandingRules::default(),
                }
            }
        };
        let rules = Arc::new(rules);
        *self.rules.lock() = Some((head, rules.clone()));
        Ok(rules)
    }

    /// What `queued`'s version changed, under `rules`.
    pub(super) async fn footprint(
        &self,
        queued: &QueuedRequest,
        rules: &LandingRules,
    ) -> Result<Footprint> {
        let paths = self
            .derive
            .get::<ChangedPaths>(&queued.request.version)
            .await?;
        Ok(Footprint::new(paths, rules))
    }
}

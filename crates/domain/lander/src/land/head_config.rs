//! The repository configuration at the head a batch lands on: landing
//! rules (where requests may run in parallel) and merge drivers (how the
//! ladder resolves conflicts). Read once per head.

use std::sync::Arc;

use bytes::Bytes;
use rill_core::RootTree;
use rill_types::{ChangeVersionHash, EntryKind, RepoPathBuf};

use super::lander::Lander;
use crate::error::Result;
use crate::ladder::{GitAttributes, MergeRules, GITATTRIBUTES_PATH, MERGE_RULES_PATH};
use crate::schedule::{LandingRules, RULES_PATH};

/// What a head's configuration says about landing on it.
#[derive(Debug, Clone, Default)]
pub struct HeadRules {
    pub landing: LandingRules,
    pub merge: MergeRules,
}

impl Lander {
    /// The rules at `head`. A file that does not parse is ignored with a
    /// warning: a bad rule file on trunk must not stop landing, including
    /// the change that fixes it.
    pub(super) async fn head_rules(
        &self,
        head: Option<ChangeVersionHash>,
    ) -> Result<Arc<HeadRules>> {
        if let Some((at, rules)) = self.rules.lock().as_ref() {
            if *at == head {
                return Ok(rules.clone());
            }
        }
        let rules = match head {
            None => HeadRules::default(),
            Some(head) => {
                let root = self.derive.get::<RootTree>(&head).await?;
                let landing = match self.read_file(root, RULES_PATH).await? {
                    Some(text) => LandingRules::parse(&String::from_utf8_lossy(&text))
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "unreadable landing rules; using the defaults");
                            LandingRules::default()
                        }),
                    None => LandingRules::default(),
                };
                let merge = match self.read_file(root, MERGE_RULES_PATH).await? {
                    Some(text) => MergeRules::parse(&String::from_utf8_lossy(&text))
                        .unwrap_or_else(|e| {
                            tracing::warn!(error = %e, "unreadable merge drivers; using none");
                            MergeRules::default()
                        }),
                    None => MergeRules::default(),
                };
                let attributes = self
                    .read_file(root, GITATTRIBUTES_PATH)
                    .await?
                    .and_then(|bytes| GitAttributes::parse(&bytes));
                HeadRules {
                    landing,
                    merge: merge.with_attributes(attributes),
                }
            }
        };
        let rules = Arc::new(rules);
        *self.rules.lock() = Some((head, rules.clone()));
        Ok(rules)
    }

    async fn read_file(&self, root: rill_types::TreeHash, path: &str) -> Result<Option<Bytes>> {
        let path = RepoPathBuf::new(path)?;
        match self.derive.trees().lookup(root, &path).await? {
            Some(EntryKind::File { content, .. }) => Ok(Some(
                rill_core::read_content(self.repo.objects().as_ref(), &content).await?,
            )),
            _ => Ok(None),
        }
    }
}

//! Marking conflicted paths resolved, refusing files that still hold
//! conflict markers so marker text is never committed as content.

use rill_types::RepoPathBuf;

use super::mount::{Change, Mount};
use crate::error::{Error, Result};

impl Mount {
    /// Clear the conflict flag of `paths`. Unless `force`, each file must
    /// parse as fully resolved.
    pub async fn resolve(&self, paths: &[RepoPathBuf], force: bool) -> Result<()> {
        self.settle().await?;
        let _op = self.ops.lock().await;
        let mut change = Change::default();
        for path in paths {
            let entry = self
                .entry(path)
                .filter(|e| e.conflict)
                .ok_or_else(|| Error::Invalid(format!("{path} is not conflicted")))?;
            if !force {
                if let Some(digest) = entry.digest() {
                    let bytes = self.shared.content.local(digest).await?;
                    match rill_core::parse_materialized(&bytes) {
                        Some(terms) if terms.is_resolved() => {}
                        Some(_) => {
                            return Err(Error::Conflict(format!(
                                "{path} still contains conflict markers"
                            )))
                        }
                        None => {
                            return Err(Error::Conflict(format!(
                                "{path} has malformed conflict markers"
                            )))
                        }
                    }
                }
            }
            change.overlay.push((path.clone(), Some((entry.op, false))));
        }
        self.write(change, None).await
    }
}

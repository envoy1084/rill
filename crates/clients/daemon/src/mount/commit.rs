//! Recording the working copy as a change version: the overlay's
//! differences from the base, applied on the base version, amending the
//! working change or starting a new one.

use rill_sdk::{ApplyOptions, EditBase, Root, VersionResult};
use rill_types::{ChangeId, IntentId, Provenance};

use super::mount::{Change, Mount};
use crate::error::{Error, Result};
use crate::overlay::ChangeKind;
use crate::sync::stored_edits;

/// What to record.
#[derive(Debug, Clone, Default)]
pub struct CommitOptions {
    /// The message; `None` keeps the working change's.
    pub message: Option<String>,
    /// The intent the change serves.
    pub intent: Option<IntentId>,
    /// Start a new change (stacked on the working change unless it has
    /// landed) instead of amending the working change.
    pub new_change: bool,
    /// Who and what produced it.
    pub provenance: Provenance,
}

/// A recorded version.
#[derive(Debug, Clone)]
pub struct Committed {
    /// The version.
    pub result: VersionResult,
    /// The change the new one is stacked on.
    pub stacked_on: Option<ChangeId>,
}

impl Mount {
    /// Record the working copy as a change version.
    pub async fn commit(&self, opts: CommitOptions) -> Result<Committed> {
        self.settle().await?;
        let _op = self.ops.lock().await;
        let mut stacked_on = None;
        let mut amend = self.record().working_change;
        if opts.new_change {
            if let Some(previous) = amend.take() {
                let current = self.repo.current_version(previous).await?;
                if current.trunk_seq.is_none() {
                    // Stack: the previous change becomes the base, so only
                    // edits made since it go into the new change.
                    let report = self.update_locked(Root::Version(current.hash)).await?;
                    if !report.conflicts.is_empty() {
                        return Err(Error::Conflict(format!(
                            "cannot start a new change: moving onto {} conflicts in {}",
                            current.hash.short(),
                            join(&report.conflicts)
                        )));
                    }
                    stacked_on = Some(previous);
                }
            }
        } else if amend.is_none() {
            return Err(Error::Invalid(
                "no working change yet: run `rill new -m <message>` first".into(),
            ));
        }
        let message = match (opts.message, amend) {
            (Some(m), _) => m,
            (None, Some(id)) => self.repo.current_version(id).await?.message,
            (None, None) => String::new(),
        };

        let changes = self.changes(&[]).await?;
        if let Some(c) = changes.iter().find(|c| c.kind == ChangeKind::Conflict) {
            return Err(Error::Conflict(format!(
                "{} has unresolved conflicts; edit it, then `rill resolve {}`",
                c.path, c.path
            )));
        }
        let entries: Vec<_> = changes
            .iter()
            .filter_map(|c| self.entry(&c.path).map(|e| (c.path.clone(), e)))
            .collect();
        let edits = stored_edits(self, &entries).await?;
        let record = self.record();
        let base = match record.base_version {
            Some(v) => Root::Version(v),
            None => Root::Tree(record.base_tree),
        };
        let mut apply = ApplyOptions::message(message).with_provenance(opts.provenance);
        if let Some(intent) = opts.intent {
            apply = apply.with_intent(intent);
        }
        apply.change_id = amend;
        let result = self
            .repo
            .apply_edits(EditBase::Root(base), edits, apply)
            .await?;
        // The version records the intent; the intent's change list is a
        // separate record only the link call updates.
        if let Some(intent) = result.version.intent {
            self.repo
                .intents()
                .link_change(intent, result.version.change_id)
                .await?;
        }
        let mut record = self.record();
        record.working_change = Some(result.version.change_id);
        self.write(
            Change {
                record: Some(record),
                ..Change::default()
            },
            None,
        )
        .await?;
        Ok(Committed { result, stacked_on })
    }
}

pub(crate) fn join(paths: &[rill_types::RepoPathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

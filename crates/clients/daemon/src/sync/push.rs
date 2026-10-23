//! Pushing the overlay to the server workspace as a snapshot built from
//! edits: only what changed since the last push, conditional on the
//! workspace's etag; the whole overlay from the base when that is unknown
//! or stale.

use rill_sdk::ErrorReason;

use super::upload::stored_edits;
use crate::error::Result;
use crate::mount::{Change, Mount, MountMode};

impl Mount {
    /// Push pending edits now; returns how many were pushed.
    pub async fn push(&self) -> Result<u64> {
        let _op = self.ops.lock().await;
        self.push_locked().await
    }

    pub(crate) async fn push_locked(&self) -> Result<u64> {
        let record = self.record();
        let Some(workspace) = record.workspace else {
            return Ok(0);
        };
        if record.mode == MountMode::Snapshot {
            return Ok(0);
        }
        let full = record.etag.is_none();
        let (entries, upto) = if full {
            let (_, upto) = self.pending();
            (self.entries(), upto)
        } else {
            self.pending()
        };
        if entries.is_empty() && !full {
            return Ok(0);
        }
        let workspaces = self.repo.workspaces();
        let edits = stored_edits(self, &entries).await?;
        let (op, pushed) = match workspaces
            .snapshot_edits(workspace, edits, full, record.etag.as_deref())
            .await
        {
            Ok(op) => (op, entries.len()),
            Err(e) if e.is(ErrorReason::EtagMismatch) => {
                tracing::warn!(mount = %record.id, "workspace changed elsewhere; rebuilding its snapshot");
                let all = self.entries();
                let edits = stored_edits(self, &all).await?;
                let op = workspaces
                    .snapshot_edits(workspace, edits, true, None)
                    .await?;
                (op, all.len())
            }
            Err(e) => return Err(e.into()),
        };
        let mut record = self.record();
        record.etag = Some(op.id.to_string());
        record.synced = upto;
        self.write(
            Change {
                record: Some(record),
                ..Change::default()
            },
            None,
        )
        .await?;
        Ok(pushed as u64)
    }
}

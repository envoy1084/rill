//! The reader thread: resolves, orders and streams commits to the writer.

use std::collections::HashSet;

use tokio::sync::mpsc;

use crate::error::Result;
use crate::reader::{history, CommitChanges, GitCommit, GitSha, GitSource};

/// What the reader thread hands to the writer.
pub(super) enum Item {
    Head(GitSha),
    Known(GitCommit),
    New(GitCommit, CommitChanges),
}

/// Body of the reader thread: resolve, order, and stream commits. Stops
/// quietly when the writer hangs up.
pub(super) fn read_loop<S: GitSource>(
    mut source: S,
    head_ref: &str,
    known: &HashSet<GitSha>,
    tx: &mpsc::Sender<Result<Item>>,
) {
    let mut produce = || -> Result<()> {
        let head = source.resolve(head_ref)?;
        if tx.blocking_send(Ok(Item::Head(head.clone()))).is_err() {
            return Ok(());
        }
        for commit in history(&mut source, &head)? {
            let item = if known.contains(&commit.sha) {
                Item::Known(commit)
            } else {
                let changes = source.changes(&commit.sha)?;
                Item::New(commit, changes)
            };
            if tx.blocking_send(Ok(item)).is_err() {
                return Ok(());
            }
        }
        Ok(())
    };
    if let Err(e) = produce() {
        // The writer may already be gone; nothing more to report then.
        let _ = tx.blocking_send(Err(e));
    }
}

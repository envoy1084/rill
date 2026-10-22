//! The write check every stored version passes, inside its commit, and
//! again when it lands: its writer may write every path it changes, read
//! every path it copies from, and own the repository's configuration to
//! change that.

use rill_policy::{Relation, Verb};
use rill_types::{ChangeVersion, RepoPath};

use rill_runtime::writes::{is_config, write_set};

use crate::auth::Permit;
use crate::services::common::Opened;
use rill_runtime::error::Result;

/// Authorize `cv` for its committer. Returns the relation version the
/// decision rests on; the commit asserts it, so a relation change racing
/// the commit forces it to be decided again.
pub async fn version(opened: &Opened, cv: &ChangeVersion) -> Result<i64> {
    let permit = opened.permit();
    let set = write_set(&opened.repo.trees, &opened.repo.derive, cv).await?;
    for path in &set.writes {
        write(&permit, path).await?;
    }
    for path in &set.copies_from {
        permit.read(path).await?;
    }
    if set.writes.is_empty() {
        permit.write_any().await?;
    }
    permit.relations_version().await
}

async fn write(permit: &Permit<'_>, path: &RepoPath) -> Result<()> {
    if is_config(path) {
        permit.check(Verb::Write, Relation::Owner, path).await
    } else {
        permit.write(path).await
    }
}

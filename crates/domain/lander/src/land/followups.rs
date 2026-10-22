//! Work a landing makes necessary, enqueued in the landing's transaction:
//! deriving background data for each landed version and indexing the
//! new trunk head for search.

use rill_jobs::{JobSpec, Priority, Queue};
use rill_store::{RepoStore, Transaction};
use rill_types::ChangeVersionHash;

use crate::error::Result;

/// Queue of versions whose background kinds need deriving.
pub const DERIVE_QUEUE: &str = "derive";
/// Queue of trunk heads to index for search.
pub const INDEX_QUEUE: &str = "index";
/// Job kind on [`DERIVE_QUEUE`].
pub const DERIVE_JOB: &str = "derive_background";
/// Job kind on [`INDEX_QUEUE`]: index a version's root tree.
pub const INDEX_JOB: &str = "index_version";
/// Job kind on [`INDEX_QUEUE`]: index a root tree a search asked for.
pub const INDEX_ROOT_JOB: &str = "index_root";

/// The job deriving `version`'s background kinds.
pub fn derive_job(version: &ChangeVersionHash) -> JobSpec {
    JobSpec::new(DERIVE_JOB, version.to_hex(), version.as_bytes().to_vec())
}

/// The job indexing `version` for search.
pub fn index_job(version: &ChangeVersionHash) -> JobSpec {
    JobSpec::new(INDEX_JOB, version.to_hex(), version.as_bytes().to_vec()).priority(Priority::Low)
}

/// The job indexing `root`, ahead of landings because a search waits on it.
pub fn index_root_job(root: &rill_types::TreeHash) -> JobSpec {
    JobSpec::new(INDEX_ROOT_JOB, root.to_hex(), root.as_bytes().to_vec()).priority(Priority::High)
}

/// The jobs for landing `versions`, in order: each is derived, and the
/// last, the new trunk head, is indexed.
pub fn landing_jobs(repo: &RepoStore, versions: &[ChangeVersionHash]) -> Result<Transaction> {
    let derive = Queue::new(repo.clone(), DERIVE_QUEUE);
    let mut tx = Transaction::new();
    for version in versions {
        tx.mutations.extend(derive.enqueue(derive_job(version))?);
    }
    if let Some(head) = versions.last() {
        let index = Queue::new(repo.clone(), INDEX_QUEUE);
        tx.mutations.extend(index.enqueue(index_job(head))?);
    }
    Ok(tx)
}

//! An org's execution space: the content store build tools see and the
//! records the scheduler keeps.

use std::sync::Arc;

use rill_store::{ObjectStore, RepoStore, Stores};
use rill_types::RepoId;

/// The namespace beside an org's own that holds its execution content.
/// It is separate so the right to execute never reads repository content
/// past its read grants; repository files enter it by copy, with no
/// rehashing (ADR 016).
pub fn content_namespace(org: &str) -> String {
    format!("{org}.exec")
}

/// One org's execution content and records.
#[derive(Clone)]
pub struct ExecSpace {
    pub org: String,
    pub objects: Arc<dyn ObjectStore>,
    /// Records (results, operations, queues) under an id no repository
    /// uses.
    pub records: RepoStore,
}

impl ExecSpace {
    pub fn open(stores: &Stores, org: &str) -> Self {
        let scoped = stores.org(org);
        ExecSpace {
            org: org.to_string(),
            objects: scoped.objects.namespaced(&content_namespace(org)),
            records: scoped.repo(RepoId::derived("rill-exec", org)),
        }
    }
}

/// The records shared by every org: worker sessions and the wake counter
/// idle workers watch.
pub fn instance_records(stores: &Stores) -> RepoStore {
    stores.repo(RepoId::derived("rill-exec", ""))
}

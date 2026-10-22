//! The handle on one repository.

use std::sync::Arc;

use bytes::Bytes;
use rill_types::{ObjectHash, ObjectKind, RepoId};

use super::stores::Stores;
use crate::error::Result;
use crate::keys::RepoKeys;
use crate::meta::MetaStore;
use crate::object::ObjectStore;

/// Typed operations on one repository.
#[derive(Clone)]
pub struct RepoStore {
    stores: Stores,
    id: RepoId,
    keys: Arc<RepoKeys>,
}

impl RepoStore {
    pub(super) fn new(stores: Stores, id: RepoId) -> Self {
        let keys = Arc::new(RepoKeys::new(stores.namespace(), id));
        RepoStore { stores, id, keys }
    }

    pub fn id(&self) -> RepoId {
        self.id
    }

    pub fn objects(&self) -> &Arc<dyn ObjectStore> {
        &self.stores.objects
    }

    pub fn meta(&self) -> &Arc<dyn MetaStore> {
        &self.stores.meta
    }

    pub fn stores(&self) -> &Stores {
        &self.stores
    }

    /// The keys of this repository's rows.
    pub fn keys(&self) -> &RepoKeys {
        &self.keys
    }

    /// Fetch raw object bytes by kind and hash.
    pub async fn get_bytes(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        self.objects().get(kind, hash).await
    }
}

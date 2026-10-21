//! The handle on instance-wide identity tables.

use std::sync::Arc;

use crate::meta::MetaStore;
use crate::repo::Stores;

/// Instance-wide identity state in the metadata store.
#[derive(Clone)]
pub struct IdentityStore {
    pub(super) meta: Arc<dyn MetaStore>,
}

impl Stores {
    /// The instance's identity state.
    pub fn identity(&self) -> IdentityStore {
        IdentityStore {
            meta: self.meta.clone(),
        }
    }
}

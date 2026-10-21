//! The handle on the audit tables.

use std::sync::Arc;

use crate::meta::MetaStore;
use crate::repo::Stores;

/// The instance's audit log.
#[derive(Clone)]
pub struct AuditStore {
    pub(super) meta: Arc<dyn MetaStore>,
}

impl Stores {
    /// The audit log.
    pub fn audit(&self) -> AuditStore {
        AuditStore {
            meta: self.meta.clone(),
        }
    }
}

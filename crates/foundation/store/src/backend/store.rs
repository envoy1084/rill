//! The backend's handle: where it writes, how it seals and how it puts.

use std::sync::Arc;

use object_store::path::Path;
use rill_types::{ObjectHash, ObjectKind};

use crate::seal::{Plaintext, Sealer, DEFAULT_NAMESPACE};

/// How writes avoid overwriting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutStrategy {
    /// `PutMode::Create`, enforced by the service.
    Conditional,
    /// A `HEAD` before an unconditional put, for services without
    /// conditional put.
    CheckThenPut,
}

/// [`ObjectStore`] over any [`object_store::ObjectStore`].
#[derive(Debug, Clone)]
pub struct ObjectStoreBackend {
    pub(super) store: Arc<dyn object_store::ObjectStore>,
    /// The prefix holding every namespace.
    pub(super) base: Path,
    pub(super) namespace: Arc<str>,
    pub(super) prefix: Path,
    pub(super) sealer: Arc<dyn Sealer>,
    pub(super) strategy: PutStrategy,
}

impl ObjectStoreBackend {
    /// Wrap `store`, placing every key under `prefix` (may be empty) and the
    /// default namespace.
    pub fn new(store: Arc<dyn object_store::ObjectStore>, prefix: impl AsRef<str>) -> Self {
        let base = Path::from(prefix.as_ref());
        ObjectStoreBackend {
            store,
            prefix: base.clone().join(DEFAULT_NAMESPACE),
            namespace: Arc::from(DEFAULT_NAMESPACE),
            base,
            sealer: Arc::new(Plaintext),
            strategy: PutStrategy::Conditional,
        }
    }

    /// Seal frames with `sealer`.
    pub fn with_sealer(mut self, sealer: Arc<dyn Sealer>) -> Self {
        self.sealer = sealer;
        self
    }

    /// The write strategy in use.
    pub fn strategy(&self) -> PutStrategy {
        self.strategy
    }

    /// Key prefix every object lives under.
    pub fn prefix(&self) -> &Path {
        &self.prefix
    }

    pub(super) fn path_for(&self, kind: ObjectKind, hash: &ObjectHash) -> Path {
        let hex = hash.to_hex();
        self.prefix
            .clone()
            .join(kind.as_str())
            .join(&hex[..2])
            .join(hex.as_str())
    }
}

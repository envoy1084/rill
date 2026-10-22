//! Detecting whether the service honours conditional put.

use bytes::Bytes;
use object_store::{PutMode, PutOptions};

use super::{ObjectStoreBackend, PutStrategy};
use crate::error::{Error, Result};

impl ObjectStoreBackend {
    /// Check whether the service honours conditional put by creating a probe
    /// object twice. Without it the backend refuses to start unless
    /// `allow_unconditional` is set, in which case it falls back to
    /// [`PutStrategy::CheckThenPut`].
    pub async fn probe(mut self, allow_unconditional: bool) -> Result<Self> {
        let path = self.prefix.clone().join(".rill-probe");
        let create = || {
            self.store.put_opts(
                &path,
                Bytes::from_static(b"probe").into(),
                PutOptions::from(PutMode::Create),
            )
        };
        // Supported means the second create of the same key is refused. An
        // unsupported mode errors either way or lets the overwrite through.
        let supported = match create().await {
            Err(e) if is_unsupported(&e) => false,
            Err(object_store::Error::AlreadyExists { .. }) | Ok(_) => match create().await {
                Err(object_store::Error::AlreadyExists { .. }) => true,
                Ok(_) => false,
                Err(e) if is_unsupported(&e) => false,
                Err(e) => return Err(Error::backend(e)),
            },
            Err(e) => return Err(Error::backend(e)),
        };
        if supported {
            self.strategy = PutStrategy::Conditional;
            return Ok(self);
        }
        if !allow_unconditional {
            return Err(Error::Backend(
                "object store does not support conditional put; set \
                 allow_unconditional_put to fall back to check-then-put"
                    .into(),
            ));
        }
        tracing::warn!("object store lacks conditional put; using check-then-put");
        self.strategy = PutStrategy::CheckThenPut;
        Ok(self)
    }
}

/// Errors a service gives for a conditional put it does not implement.
fn is_unsupported(e: &object_store::Error) -> bool {
    matches!(
        e,
        object_store::Error::NotImplemented { .. } | object_store::Error::NotSupported { .. }
    )
}

//! What a key service does for Rill.

use async_trait::async_trait;
use rill_types::identity::WrappedSecret;

use crate::error::Result;

/// Encrypts and decrypts key secrets.
#[async_trait]
pub trait KeyWrapper: Send + Sync {
    /// The backend name stored with each wrapped secret.
    fn backend(&self) -> &'static str;

    /// The wrapping key within the backend.
    fn key_ref(&self) -> &str;

    async fn wrap(&self, plaintext: &[u8]) -> Result<WrappedSecret>;

    async fn unwrap(&self, wrapped: &WrappedSecret) -> Result<Vec<u8>>;

    /// Whether this service can unwrap `wrapped`.
    fn can_unwrap(&self, wrapped: &WrappedSecret) -> bool {
        wrapped.backend == self.backend() && wrapped.key_ref == self.key_ref()
    }
}

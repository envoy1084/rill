//! Reading the bytes behind a content reference, from any source.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_store::ObjectStore;
use rill_types::ContentRef;

use super::read_content;
use crate::error::Result;

/// Something that can produce the bytes behind a [`ContentRef`].
#[async_trait]
pub trait BlobReader: Send + Sync {
    /// Read the full content of `content`.
    async fn read(&self, content: &ContentRef) -> Result<Bytes>;
}

#[async_trait]
impl BlobReader for Arc<dyn ObjectStore> {
    async fn read(&self, content: &ContentRef) -> Result<Bytes> {
        read_content(self.as_ref(), content).await
    }
}

#[async_trait]
impl<T: BlobReader + ?Sized> BlobReader for Arc<T> {
    async fn read(&self, content: &ContentRef) -> Result<Bytes> {
        (**self).read(content).await
    }
}

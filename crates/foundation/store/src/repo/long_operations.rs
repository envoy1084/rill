//! Long-running operations: work that outlives the request that started
//! it. The record is opaque here; the API encodes it. Rows are sealed
//! like the misc rows, since they name changes and paths.

use bytes::Bytes;
use rill_types::OperationId;

use super::store::RepoStore;
use crate::error::Result;
use crate::keys::table;
use crate::meta::KeyRange;

impl RepoStore {
    /// Store or replace operation `id`.
    pub async fn put_long_operation(&self, id: &OperationId, record: Bytes) -> Result<()> {
        let row = self.seal_row(record).await?;
        self.meta().set(self.keys().long_operation(id), row).await
    }

    /// Operation `id`, if stored.
    pub async fn long_operation(&self, id: &OperationId) -> Result<Option<Bytes>> {
        let row = self.meta().get(&self.keys().long_operation(id)).await?;
        self.open_row(row).await
    }

    /// Every stored operation, in id order.
    pub async fn long_operations(&self) -> Result<Vec<Bytes>> {
        let prefix = self.keys().table(table::LONG_OPERATION);
        let mut out = Vec::new();
        for (_, row) in self.meta().scan(KeyRange::prefix(prefix)).await? {
            out.extend(self.open_row(Some(row)).await?);
        }
        Ok(out)
    }
}

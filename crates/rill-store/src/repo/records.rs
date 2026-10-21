//! Workspaces, operations and intents.

use bytes::Bytes;
use rill_types::{Intent, IntentId, Operation, OperationId, Workspace, WorkspaceId};

use super::codec::{decode_record, encode_record};
use super::store::RepoStore;
use crate::error::Result;
use crate::keys::table;
use crate::meta::{KeyRange, Transaction};

impl RepoStore {
    pub async fn get_workspace(&self, id: &WorkspaceId) -> Result<Option<Workspace>> {
        match self.meta().get(&self.keys().workspace(id)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn put_workspace(&self, ws: &Workspace) -> Result<()> {
        self.meta()
            .set(self.keys().workspace(&ws.id), encode_record(ws)?)
            .await
    }

    pub async fn delete_workspace(&self, id: &WorkspaceId) -> Result<()> {
        self.meta()
            .transact(Transaction::new().delete(self.keys().workspace(id)))
            .await?;
        Ok(())
    }

    pub async fn list_workspaces(&self) -> Result<Vec<Workspace>> {
        self.list_records(table::WORKSPACE).await
    }

    /// Append an operation and write its resulting workspace in one
    /// transaction, failing with a conflict if the operation id was
    /// already applied or the workspace changed since `expected` was read
    /// (`None`: the workspace must not exist yet).
    pub async fn append_operation(
        &self,
        expected: Option<&Workspace>,
        op: &Operation,
    ) -> Result<()> {
        let tx = self.operation_tx(expected, op)?;
        self.meta().transact(tx).await?;
        Ok(())
    }

    /// The transaction [`RepoStore::append_operation`] commits, to merge
    /// into a larger one.
    pub fn operation_tx(
        &self,
        expected: Option<&Workspace>,
        op: &Operation,
    ) -> Result<Transaction> {
        let keys = self.keys();
        let index = keys.operation_index(&op.id);
        let ws_key = keys.workspace(&op.workspace);
        Ok(Transaction::new()
            .expect_absent(index.clone())
            .expect(ws_key.clone(), expected.map(encode_record).transpose()?)
            .append_stamped(keys.operations(&op.workspace), encode_record(op)?)
            .set(ws_key, encode_record(&op.resulting)?)
            .set(index, Bytes::new()))
    }

    pub async fn has_operation(&self, op: &OperationId) -> Result<bool> {
        Ok(self
            .meta()
            .get(&self.keys().operation_index(op))
            .await?
            .is_some())
    }

    /// The newest `limit` operations of a workspace, oldest first.
    pub async fn list_operations(&self, ws: &WorkspaceId, limit: usize) -> Result<Vec<Operation>> {
        let rows = self
            .meta()
            .scan(
                KeyRange::prefix(self.keys().operations(ws))
                    .reverse()
                    .limit(limit),
            )
            .await?;
        let mut ops: Vec<Operation> = rows
            .iter()
            .map(|(_, v)| decode_record(v))
            .collect::<Result<_>>()?;
        ops.reverse();
        Ok(ops)
    }

    pub async fn get_intent(&self, id: &IntentId) -> Result<Option<Intent>> {
        match self.meta().get(&self.keys().intent(id)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn put_intent(&self, intent: &Intent) -> Result<()> {
        self.meta()
            .set(self.keys().intent(&intent.id), encode_record(intent)?)
            .await
    }

    pub async fn list_intents(&self) -> Result<Vec<Intent>> {
        self.list_records(table::INTENT).await
    }

    pub(super) async fn list_records<T: rill_types::Record>(&self, table: &str) -> Result<Vec<T>> {
        let rows = self
            .meta()
            .scan(KeyRange::prefix(self.keys().table(table)))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }
}

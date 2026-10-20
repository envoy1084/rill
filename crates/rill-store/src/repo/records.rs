//! Review states, workspaces, operations and intents.

use bytes::Bytes;
use rill_types::{
    ChangeId, Intent, IntentId, Operation, OperationId, ReviewState, Workspace, WorkspaceId,
};

use super::codec::{decode_record, encode_record};
use super::store::RepoStore;
use crate::error::Result;
use crate::keys::table;
use crate::meta::{KeyRange, Transaction};

impl RepoStore {
    pub async fn get_review(&self, id: &ChangeId) -> Result<Option<ReviewState>> {
        match self.meta().get(&self.keys().change(id)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Write a review state. When `expected` is given, the write succeeds
    /// only if the stored record still equals it (optimistic concurrency).
    pub async fn put_review(
        &self,
        state: &ReviewState,
        expected: Option<&ReviewState>,
    ) -> Result<()> {
        let key = self.keys().change(&state.change_id);
        let mut tx = Transaction::new();
        if let Some(prev) = expected {
            tx = tx.expect(key.clone(), Some(encode_record(prev)?));
        }
        tx = tx.set(key, encode_record(state)?);
        self.meta().transact(tx).await?;
        Ok(())
    }

    pub async fn list_reviews(&self) -> Result<Vec<ReviewState>> {
        self.list_records(table::CHANGE).await
    }

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

    /// Append an operation and update the workspace in one transaction,
    /// failing with a conflict if the operation id was already applied.
    /// Returns the operation's sequence within the workspace log.
    pub async fn append_operation(&self, ws: &Workspace, op: &Operation) -> Result<u64> {
        let index = self.keys().operation_index(&op.id);
        let tx = Transaction::new()
            .expect_absent(index.clone())
            .append_log(self.keys().operations(&ws.id), encode_record(op)?)
            .set(self.keys().workspace(&ws.id), encode_record(ws)?)
            .set(index, Bytes::new());
        let out = self.meta().transact(tx).await?;
        Ok(out.appended.first().copied().unwrap_or(0))
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

    async fn list_records<T: rill_types::Record>(&self, table: &str) -> Result<Vec<T>> {
        let rows = self
            .meta()
            .scan(KeyRange::prefix(self.keys().table(table)))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }
}

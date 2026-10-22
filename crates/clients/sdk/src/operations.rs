//! Long-running operations on one repository.

use std::time::Duration;

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;

use crate::client::{retry_idempotent, Client};
use crate::error::Result;
use crate::paging;

/// One repository's long-running operations.
#[derive(Clone, Debug)]
pub struct OperationClient {
    client: Client,
    /// The repository's full resource name.
    repo: String,
}

impl OperationClient {
    pub(crate) fn new(client: Client, repo: String) -> Self {
        OperationClient { client, repo }
    }

    fn svc(&self) -> crate::client::OperationSvc {
        self.client.operation_svc()
    }

    /// The full name of operation `id_or_name`.
    fn name(&self, id_or_name: &str) -> String {
        match id_or_name.contains('/') {
            true => id_or_name.to_string(),
            false => format!("{}/operations/{id_or_name}", self.repo),
        }
    }

    /// An operation, by id or name.
    pub async fn get(&self, operation: &str) -> Result<pb::Operation> {
        let name = self.name(operation);
        let resp = retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetOperationRequest { name: name.clone() };
            async move { svc.get_operation(request).await }
        })
        .await?
        .into_owned();
        Ok(required("get_operation.operation", resp.operation)?)
    }

    /// Wait up to `timeout` (the server holds a wait at most 60 s) for an
    /// operation to finish; it as it stands then.
    pub async fn wait(&self, operation: &str, timeout: Duration) -> Result<pb::Operation> {
        let name = self.name(operation);
        let resp = retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::WaitOperationRequest {
                name: name.clone(),
                timeout_ms: timeout.as_millis().min(u32::MAX as u128) as u32,
            };
            async move { svc.wait_operation(request).await }
        })
        .await?
        .into_owned();
        Ok(required("wait_operation.operation", resp.operation)?)
    }

    /// Every operation, newest first; `filter` on `kind` and `done`.
    pub async fn list(&self, filter: &str) -> Result<Vec<pb::Operation>> {
        paging::all(|page_token| async move {
            let request = pb::ListOperationsRequest {
                parent: self.repo.clone(),
                page_size: paging::PAGE,
                page_token,
                filter: filter.to_string(),
            };
            let resp = retry_idempotent(|| {
                let (svc, request) = (self.svc(), request.clone());
                async move { svc.list_operations(request).await }
            })
            .await?
            .into_owned();
            Ok((resp.operations, resp.next_page_token))
        })
        .await
    }
}

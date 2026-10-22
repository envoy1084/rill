//! Reading, listing and waiting on operations.

use std::time::Duration;

use rill_proto::rill::v1 as pb;
use rill_types::RepoPathBuf;

use super::record::{all, load};
use super::OperationSvc;
use crate::listing::{FieldValue, Filter, Kind, Page, Record};
use crate::services::common::open_repo;
use crate::services::{names, Call};
use rill_runtime::error::Result;

/// The longest a wait holds the request.
const MAX_WAIT: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(250);

impl OperationSvc {
    pub(super) async fn get_operation_impl(
        &self,
        request: Call<pb::GetOperationRequest>,
    ) -> Result<pb::GetOperationResponse> {
        let name = &request.get_ref().name;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let id = names::operation_in(&opened.repo, name)?;
        Ok(pb::GetOperationResponse {
            operation: load(&opened.repo, &id).await?.into(),
        })
    }

    pub(super) async fn list_operations_impl(
        &self,
        request: Call<pb::ListOperationsRequest>,
    ) -> Result<pb::ListOperationsResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let filter = Filter::parse(&req.filter, &[("kind", Kind::Str), ("done", Kind::Bool)])?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.filter])?;
        let mut operations = all(&opened.repo).await?;
        operations.retain(|o| filter.matches(&Listed(o)));
        let (operations, next_page_token) = page.of(operations)?;
        Ok(pb::ListOperationsResponse {
            operations,
            next_page_token,
        })
    }

    pub(super) async fn wait_operation_impl(
        &self,
        request: Call<pb::WaitOperationRequest>,
    ) -> Result<pb::WaitOperationResponse> {
        let req = request.get_ref();
        let opened = open_repo(&self.state, &request, &req.name).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let id = names::operation_in(&opened.repo, &req.name)?;
        let timeout = match req.timeout_ms {
            0 => MAX_WAIT,
            ms => Duration::from_millis(ms as u64).min(MAX_WAIT),
        };
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let operation = load(&opened.repo, &id).await?;
            if operation.done || tokio::time::Instant::now() >= deadline {
                return Ok(pb::WaitOperationResponse {
                    operation: operation.into(),
                });
            }
            tokio::time::sleep(POLL.min(deadline - tokio::time::Instant::now())).await;
        }
    }
}

/// An operation as a list filter sees it.
struct Listed<'a>(&'a pb::Operation);

impl Record for Listed<'_> {
    fn field(&self, field: &str) -> FieldValue {
        match field {
            "kind" => FieldValue::Str(self.0.kind.clone()),
            "done" => FieldValue::Bool(self.0.done),
            _ => FieldValue::Missing,
        }
    }
}

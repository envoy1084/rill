//! Interdiffs between versions, applying suggested edits, and reopening
//! abandoned changes.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, ChangeVersionHash, OperationId, ReviewState};

use crate::error::Result;
use crate::names;
use crate::review::ReviewClient;

impl ReviewClient {
    /// What the author changed from `from` to `to` (default: the version
    /// before the current one, and the current one).
    pub async fn interdiff(
        &self,
        change_id: ChangeId,
        from: Option<ChangeVersionHash>,
        to: Option<ChangeVersionHash>,
    ) -> Result<pb::GetInterdiffResponse> {
        let version = |v: Option<ChangeVersionHash>| {
            v.map(|v| names::version(self.repo_name(), &v))
                .unwrap_or_default()
        };
        let request = pb::GetInterdiffRequest {
            change: self.change_name(&change_id),
            from: version(from),
            to: version(to),
        };
        Ok(crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.get_interdiff(request).await }
        })
        .await?
        .into_owned())
    }

    /// Apply thread `thread_id`'s suggested edit as a new version.
    pub async fn apply_suggestion(
        &self,
        change_id: ChangeId,
        thread_id: &str,
    ) -> Result<ChangeVersionHash> {
        let request = pb::ApplySuggestionRequest {
            request_id: names::request_id(&OperationId::new()),
            thread: self.thread(&change_id, thread_id),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.apply_suggestion(request).await }
        })
        .await?
        .into_owned();
        let info = required("apply_suggestion.info", resp.info)?;
        Ok(rill_proto::convert::hash_from_proto(
            "info.hash",
            &info.hash,
        )?)
    }

    /// Reopen an abandoned change.
    pub async fn reopen(&self, change_id: ChangeId) -> Result<ReviewState> {
        let request = pb::ReopenChangeRequest {
            request_id: names::request_id(&OperationId::new()),
            change: self.change_name(&change_id),
        };
        let resp = crate::client::retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.reopen_change(request).await }
        })
        .await?
        .into_owned();
        Ok(required("reopen_change.review", resp.review)?.try_into()?)
    }
}

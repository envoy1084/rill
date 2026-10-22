//! Derived data: server-computed artifacts keyed by kind and hash.

use std::time::Duration;

use bytes::Bytes;
use rill_proto::convert::hash_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::ObjectHash;

use super::RepoClient;
use crate::error::Result;
use crate::types::Derived;

impl RepoClient {
    /// Derived data of `kind` for `hash`. Fails with `NotFound` or
    /// `Unavailable` when it has not been computed yet.
    pub async fn derived(&self, kind: &str, hash: ObjectHash) -> Result<Bytes> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetDerivedValueRequest {
                repo: self.resource.clone(),
                kind: kind.to_string(),
                hash: hash_to_proto(&hash),
            };
            async move { svc.get_derived_value(request).await }
        })
        .await?
        .into_owned();
        Ok(resp.data)
    }

    /// Wait up to `timeout` for derived data to be computed.
    pub async fn wait_derived(
        &self,
        kind: &str,
        hash: ObjectHash,
        timeout: Duration,
    ) -> Result<Derived> {
        let resp = crate::client::retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::WaitDerivedValueRequest {
                repo: self.resource.clone(),
                kind: kind.to_string(),
                hash: hash_to_proto(&hash),
                timeout_ms: timeout.as_millis() as u64,
            };
            async move { svc.wait_derived_value(request).await }
        })
        .await?
        .into_owned();
        match (resp.ready, resp.data) {
            (true, Some(data)) => Ok(Derived::Ready(data)),
            (true, None) => Ok(Derived::Ready(Bytes::new())),
            (false, _) => Ok(Derived::Pending),
        }
    }
}

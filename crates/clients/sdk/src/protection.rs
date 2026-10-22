//! Data protection: redactions, releasing quarantined versions and
//! rotating data keys.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersionHash, ObjectHash, ObjectKind, OperationId};

use crate::client::{retry_idempotent, Client};
use crate::error::Result;
use crate::{names, paging};

/// One org's redactions and data keys (org administrators; rotating keys
/// needs an instance administrator).
#[derive(Clone, Debug)]
pub struct ProtectionClient {
    client: Client,
    /// The org's resource name.
    org: String,
}

impl ProtectionClient {
    pub(crate) fn new(client: Client, org: &str) -> Self {
        ProtectionClient {
            client,
            org: names::org(org),
        }
    }

    fn svc(&self) -> crate::client::ProtectionSvc {
        self.client.protection_svc()
    }

    /// Ask for an object to be redacted; another administrator approves.
    pub async fn request_redaction(
        &self,
        kind: ObjectKind,
        hash: ObjectHash,
        reason: &str,
    ) -> Result<pb::Redaction> {
        let request = pb::CreateRedactionRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: self.org.clone(),
            kind: pb::ObjectKind::from(kind).into(),
            hash: rill_proto::convert::hash_to_proto(&hash),
            reason: reason.to_string(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_redaction(request).await }
        })
        .await?
        .into_owned();
        Ok(required("create_redaction.redaction", resp.redaction)?)
    }

    /// Apply redaction `id`; the caller must not be who requested it.
    pub async fn approve_redaction(&self, id: &str) -> Result<pb::Redaction> {
        let request = pb::ApproveRedactionRequest {
            request_id: names::request_id(&OperationId::new()),
            name: format!("{}/redactions/{id}", self.org),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.approve_redaction(request).await }
        })
        .await?
        .into_owned();
        Ok(required("approve_redaction.redaction", resp.redaction)?)
    }

    /// Every redaction, pending and applied.
    pub async fn redactions(&self) -> Result<Vec<pb::Redaction>> {
        paging::all(|page_token| async move {
            let request = pb::ListRedactionsRequest {
                parent: self.org.clone(),
                page_size: paging::PAGE,
                page_token,
            };
            let resp = retry_idempotent(|| {
                let (svc, request) = (self.svc(), request.clone());
                async move { svc.list_redactions(request).await }
            })
            .await?
            .into_owned();
            Ok((resp.redactions, resp.next_page_token))
        })
        .await
    }

    /// Release a version of `repo` that secret scanning quarantined, so it
    /// may land (repository owners).
    pub async fn release_version(&self, repo: &str, version: &ChangeVersionHash) -> Result<()> {
        let request = pb::ReleaseVersionRequest {
            request_id: names::request_id(&OperationId::new()),
            version: names::version(&names::repo(repo), version),
        };
        retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.release_version(request).await }
        })
        .await?;
        Ok(())
    }

    /// Add a data key to the org, or with `rewrap` wrap every data key
    /// again under the key service's current key.
    pub async fn rotate_data_keys(&self, rewrap: bool) -> Result<pb::RotateDataKeysResponse> {
        let request = pb::RotateDataKeysRequest {
            request_id: names::request_id(&OperationId::new()),
            parent: self.org.clone(),
            rewrap,
        };
        Ok(retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.rotate_data_keys(request).await }
        })
        .await?
        .into_owned())
    }
}

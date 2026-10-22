//! Org secrets that verify plans give their steps; values are write-only.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;
use rill_types::OperationId;

use crate::client::{retry_idempotent, Client};
use crate::error::Result;
use crate::{names, paging};

/// One org's secrets (org administrators).
#[derive(Clone, Debug)]
pub struct SecretClient {
    client: Client,
    /// The org's resource name.
    org: String,
}

impl SecretClient {
    pub(crate) fn new(client: Client, org: &str) -> Self {
        SecretClient {
            client,
            org: names::org(org),
        }
    }

    fn svc(&self) -> crate::client::SecretSvc {
        self.client.secret_svc()
    }

    /// Set secret `name` to `value`, creating it if absent, for `repos`
    /// (empty: every repository) and, with `trunk_only`, only verification
    /// after landing.
    pub async fn set(
        &self,
        name: &str,
        value: &[u8],
        repos: Vec<String>,
        trunk_only: bool,
    ) -> Result<pb::Secret> {
        let request = pb::UpdateSecretRequest {
            request_id: String::new(),
            secret: pb::Secret {
                name: format!("{}/secrets/{name}", self.org),
                repos,
                trunk_only,
                ..Default::default()
            }
            .into(),
            value: bytes::Bytes::copy_from_slice(value),
            allow_missing: true,
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.update_secret(request).await }
        })
        .await?
        .into_owned();
        Ok(required("update_secret.secret", resp.secret)?)
    }

    /// Delete secret `name`.
    pub async fn delete(&self, name: &str) -> Result<()> {
        let request = pb::DeleteSecretRequest {
            request_id: names::request_id(&OperationId::new()),
            name: format!("{}/secrets/{name}", self.org),
        };
        retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.delete_secret(request).await }
        })
        .await?;
        Ok(())
    }

    /// Every secret's name and scope.
    pub async fn list(&self) -> Result<Vec<pb::Secret>> {
        paging::all(|page_token| async move {
            let request = pb::ListSecretsRequest {
                parent: self.org.clone(),
                page_size: paging::PAGE,
                page_token,
            };
            let resp = retry_idempotent(|| {
                let (svc, request) = (self.svc(), request.clone());
                async move { svc.list_secrets(request).await }
            })
            .await?
            .into_owned();
            Ok((resp.secrets, resp.next_page_token))
        })
        .await
    }
}

//! An org's sign-in providers, trusted workload issuers and SCIM
//! credentials.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;

use super::IdentityClient;
use crate::client::retry_idempotent;
use crate::error::Result;
use crate::paging;

impl IdentityClient {
    /// Create or replace an OpenID Connect or SAML provider; `spec` names
    /// the org and the provider.
    pub async fn put_identity_provider(
        &self,
        spec: pb::IdentityProviderSpec,
    ) -> Result<pb::IdentityProviderInfo> {
        let request = pb::UpdateIdentityProviderRequest {
            request_id: String::new(),
            provider: spec.into(),
            allow_missing: true,
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.update_identity_provider(request).await }
        })
        .await?
        .into_owned();
        Ok(required(
            "update_identity_provider.provider",
            resp.provider,
        )?)
    }

    /// The org's identity providers.
    pub async fn identity_providers(&self) -> Result<Vec<pb::IdentityProviderInfo>> {
        paging::all(|page_token| async move {
            let request = pb::ListIdentityProvidersRequest {
                parent: self.org.clone(),
                page_size: paging::PAGE,
                page_token,
            };
            let resp = retry_idempotent(|| {
                let (svc, request) = (self.svc(), request.clone());
                async move { svc.list_identity_providers(request).await }
            })
            .await?
            .into_owned();
            Ok((resp.providers, resp.next_page_token))
        })
        .await
    }

    /// Remove identity provider `name`.
    pub async fn delete_identity_provider(&self, name: &str) -> Result<()> {
        let request = pb::DeleteIdentityProviderRequest {
            request_id: Self::request_id(),
            name: self.child("identityProviders", name),
        };
        retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.delete_identity_provider(request).await }
        })
        .await?;
        Ok(())
    }

    /// Trust (or replace) an issuer of workload JWTs; `spec` names the org
    /// and the issuer.
    pub async fn put_workload_issuer(
        &self,
        spec: pb::WorkloadIssuerSpec,
    ) -> Result<pb::WorkloadIssuerSpec> {
        let request = pb::UpdateWorkloadIssuerRequest {
            request_id: Self::request_id(),
            issuer: spec.into(),
            allow_missing: true,
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.update_workload_issuer(request).await }
        })
        .await?
        .into_owned();
        Ok(required("update_workload_issuer.issuer", resp.issuer)?)
    }

    /// The org's trusted workload issuers.
    pub async fn workload_issuers(&self) -> Result<Vec<pb::WorkloadIssuerSpec>> {
        paging::all(|page_token| async move {
            let request = pb::ListWorkloadIssuersRequest {
                parent: self.org.clone(),
                page_size: paging::PAGE,
                page_token,
            };
            let resp = retry_idempotent(|| {
                let (svc, request) = (self.svc(), request.clone());
                async move { svc.list_workload_issuers(request).await }
            })
            .await?
            .into_owned();
            Ok((resp.issuers, resp.next_page_token))
        })
        .await
    }

    /// Stop trusting workload issuer `name`.
    pub async fn delete_workload_issuer(&self, name: &str) -> Result<()> {
        let request = pb::DeleteWorkloadIssuerRequest {
            request_id: Self::request_id(),
            name: self.child("workloadIssuers", name),
        };
        retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.delete_workload_issuer(request).await }
        })
        .await?;
        Ok(())
    }

    /// A bearer credential for the org's SCIM endpoint, and its token,
    /// which is shown only now.
    pub async fn create_scim_credential(&self) -> Result<(pb::ScimCredential, String)> {
        let request = pb::CreateScimCredentialRequest {
            request_id: Self::request_id(),
            parent: self.org.clone(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_scim_credential(request).await }
        })
        .await?
        .into_owned();
        Ok((
            required("create_scim_credential.credential", resp.credential)?,
            resp.token,
        ))
    }

    /// The org's SCIM credentials, revoked ones included.
    pub async fn scim_credentials(&self) -> Result<Vec<pb::ScimCredential>> {
        paging::all(|page_token| async move {
            let request = pb::ListScimCredentialsRequest {
                parent: self.org.clone(),
                page_size: paging::PAGE,
                page_token,
            };
            let resp = retry_idempotent(|| {
                let (svc, request) = (self.svc(), request.clone());
                async move { svc.list_scim_credentials(request).await }
            })
            .await?
            .into_owned();
            Ok((resp.credentials, resp.next_page_token))
        })
        .await
    }

    /// Revoke SCIM credential `id`.
    pub async fn revoke_scim_credential(&self, id: &str) -> Result<()> {
        let request = pb::DeleteScimCredentialRequest {
            request_id: Self::request_id(),
            name: self.child("scimCredentials", id),
        };
        retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.delete_scim_credential(request).await }
        })
        .await?;
        Ok(())
    }
}

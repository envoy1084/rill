//! An org's guardrails: kept in its identity settings, applied by every
//! repository of the org, which reloads its policy when they change.

use rill_policy::submit::validate_guardrails;
use rill_proto::rill::v1 as pb;
use rill_runtime::error::Result;
use rill_runtime::state::policy;
use rill_types::identity::IdentityChange;

use super::PolicySvc;
use crate::auth::{caller, require_org_admin};
use crate::services::{names, Call};

impl PolicySvc {
    pub(super) async fn get_guardrails_impl(
        &self,
        request: Call<pb::GetGuardrailsRequest>,
    ) -> Result<pb::GetGuardrailsResponse> {
        let org = names::org(&request.get_ref().org)?;
        let ctx = caller(&request)?;
        require_org_admin(&self.state, &ctx, &org).await?;
        let settings = self.state.identity.org_settings(&org).await?;
        Ok(pb::GetGuardrailsResponse {
            text: settings.guardrails.clone().unwrap_or_default(),
        })
    }

    pub(super) async fn update_guardrails_impl(
        &self,
        request: Call<pb::UpdateGuardrailsRequest>,
    ) -> Result<pb::UpdateGuardrailsResponse> {
        let req = request.get_ref();
        let org = names::org(&req.org)?;
        let ctx = caller(&request)?;
        require_org_admin(&self.state, &ctx, &org).await?;
        let text = req.text.trim().to_string();
        if !text.is_empty() {
            validate_guardrails(&text)?;
        }
        let store = self.state.identity.store();
        let mut settings = store.org_settings(&org).await?;
        settings.guardrails = (!text.is_empty()).then(|| text.clone());
        store.put_org_settings(&settings, &ctx.principal).await?;
        self.state
            .identity
            .apply(&IdentityChange::OrgSettingsChanged { org: org.clone() });
        for repo in self.state.stores.org(&org).list_repos().await? {
            let ctx_repo = self.state.repos.get(repo.id).await?;
            policy::announce(&ctx_repo.repo, policy::GUARDRAILS, &ctx.principal).await?;
        }
        tracing::info!(org = %org, by = %ctx.principal, "guardrails updated");
        Ok(pb::UpdateGuardrailsResponse { text })
    }
}

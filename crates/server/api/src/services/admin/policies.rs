//! Reading and replacing a repository's submit and risk policies.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{RepoPathBuf, Timestamp};

use super::scopes::parse_policy_name;
use super::AdminSvc;
use crate::services::Call;
use rill_runtime::error::Result;
use rill_runtime::state::policy::{self, StoredPolicy};

impl AdminSvc {
    pub(super) async fn get_policy_impl(
        &self,
        request: Call<pb::GetPolicyRequest>,
    ) -> Result<pb::GetPolicyResponse> {
        let (repo, name) = parse_policy_name(&request.get_ref().name)?;
        let opened = self.open_named(&request, &repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let stored = match policy::load(&opened.repo.repo, &name).await? {
            Some(stored) => stored,
            None => StoredPolicy {
                text: policy::default_text(&name, &self.state.config.risk)?,
                updated_at: opened.repo.config.created_at,
                updated_by: rill_lander::revert::lander_principal(),
            },
        };
        Ok(pb::GetPolicyResponse {
            name: request.get_ref().name.clone(),
            text: stored.text,
            updated_at: timestamp_to_proto(stored.updated_at),
            updated_by: pb::Principal::from(stored.updated_by).into(),
        })
    }

    pub(super) async fn set_policy_impl(
        &self,
        request: Call<pb::SetPolicyRequest>,
    ) -> Result<pb::SetPolicyResponse> {
        let (repo, name) = parse_policy_name(&request.get_ref().name)?;
        let opened = self.open_named(&request, &repo).await?;
        opened.permit().admin().await?;
        let text = request.get_ref().text.clone();
        policy::validate(&name, &text)?;
        let stored = StoredPolicy {
            text,
            updated_at: Timestamp::now(),
            updated_by: opened.principal().clone(),
        };
        policy::store(&opened.repo.repo, &name, &stored).await?;
        opened.repo.reload_policy().await?;
        Ok(pb::SetPolicyResponse {
            updated_at: timestamp_to_proto(stored.updated_at),
        })
    }
}

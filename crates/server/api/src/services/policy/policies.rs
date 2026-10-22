//! Reading and replacing a repository's submit and risk policies.

use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{RepoPathBuf, Timestamp};

use super::PolicySvc;
use crate::services::common::{open_repo, Opened};
use crate::services::{etag, names, Call};
use rill_runtime::error::{Error, Result};
use rill_runtime::state::policy::{self, StoredPolicy};

/// The policy a policy name names, in the repository it is in.
async fn named(
    svc: &PolicySvc,
    request: &Call<impl Sized>,
    name: &str,
) -> Result<(Opened, String)> {
    let opened = open_repo(&svc.state, request, name).await?;
    let parts = names::parse(&rill_proto::names::POLICY, name)?;
    let policy = parts[2].to_string();
    if policy != policy::SUBMIT_POLICY && policy != policy::RISK_POLICY {
        return Err(Error::InvalidName(format!(
            "{name:?} names no policy; repositories have {} and {}",
            policy::SUBMIT_POLICY,
            policy::RISK_POLICY
        )));
    }
    Ok((opened, policy))
}

/// The stored policy, or its default text when never set.
async fn current(svc: &PolicySvc, opened: &Opened, name: &str, policy: &str) -> Result<pb::Policy> {
    let stored = match policy::load(&opened.repo.repo, policy).await? {
        Some(stored) => stored,
        None => StoredPolicy {
            text: policy::default_text(policy, &svc.state.config.risk)?,
            updated_at: opened.repo.config.created_at,
            updated_by: rill_lander::revert::lander_principal(),
        },
    };
    Ok(policy_proto(name, stored))
}

fn policy_proto(name: &str, stored: StoredPolicy) -> pb::Policy {
    let mut out = pb::Policy {
        name: name.to_string(),
        text: stored.text,
        updated_at: timestamp_to_proto(stored.updated_at),
        updated_by: pb::Principal::from(stored.updated_by).into(),
        etag: String::new(),
    };
    out.etag = etag::of(&out);
    out
}

impl PolicySvc {
    pub(super) async fn get_policy_impl(
        &self,
        request: Call<pb::GetPolicyRequest>,
    ) -> Result<pb::GetPolicyResponse> {
        let name = &request.get_ref().name;
        let (opened, policy) = named(self, &request, name).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        Ok(pb::GetPolicyResponse {
            policy: current(self, &opened, name, &policy).await?.into(),
        })
    }

    /// Validate, store and hot-swap a policy.
    pub(super) async fn update_policy_impl(
        &self,
        request: Call<pb::UpdatePolicyRequest>,
    ) -> Result<pb::UpdatePolicyResponse> {
        let given = request
            .get_ref()
            .policy
            .as_option()
            .ok_or(rill_proto::ConvertError::missing("policy"))?
            .clone();
        let (opened, policy) = named(self, &request, &given.name).await?;
        opened.permit().admin().await?;
        let before = current(self, &opened, &given.name, &policy).await?;
        etag::check(&before.etag, &request.get_ref().etag, "the policy")?;
        policy::validate(&policy, &given.text)?;
        let stored = StoredPolicy {
            text: given.text,
            updated_at: Timestamp::now(),
            updated_by: opened.principal().clone(),
        };
        policy::store(&opened.repo.repo, &policy, &stored).await?;
        opened.repo.reload_policy().await?;
        Ok(pb::UpdatePolicyResponse {
            policy: policy_proto(&given.name, stored).into(),
        })
    }
}

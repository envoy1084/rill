//! An org's agents: org administrators, or a user for agents they sponsor.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;

use super::{mask, IdentityClient};
use crate::client::retry_idempotent;
use crate::error::Result;
use crate::paging;

impl IdentityClient {
    /// Register an agent; `spec.org` must be this org.
    pub async fn create_agent(&self, spec: pb::AgentSpec) -> Result<pb::Agent> {
        let request = pb::CreateAgentRequest {
            request_id: Self::request_id(),
            parent: self.org.clone(),
            agent: spec.into(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.create_agent(request).await }
        })
        .await?
        .into_owned();
        Ok(required("create_agent.agent", resp.agent)?)
    }

    /// Agent `name`.
    pub async fn agent(&self, name: &str) -> Result<pb::Agent> {
        let name = self.child("agents", name);
        let resp = retry_idempotent(|| {
            let svc = self.svc();
            let request = pb::GetAgentRequest { name: name.clone() };
            async move { svc.get_agent(request).await }
        })
        .await?
        .into_owned();
        Ok(required("get_agent.agent", resp.agent)?)
    }

    /// The agents the caller may see, matching `filter` (on `status`,
    /// `sponsor`; empty for all).
    pub async fn agents(&self, filter: &str) -> Result<Vec<pb::Agent>> {
        paging::all(|page_token| async move {
            let request = pb::ListAgentsRequest {
                parent: self.org.clone(),
                page_size: paging::PAGE,
                page_token,
                filter: filter.to_string(),
            };
            let resp = retry_idempotent(|| {
                let (svc, request) = (self.svc(), request.clone());
                async move { svc.list_agents(request).await }
            })
            .await?
            .into_owned();
            Ok((resp.agents, resp.next_page_token))
        })
        .await
    }

    /// Suspend or reactivate agent `name`.
    pub async fn set_agent_suspended(&self, name: &str, suspended: bool) -> Result<pb::Agent> {
        let agent = pb::Agent {
            name: self.child("agents", name),
            status: match suspended {
                true => "suspended",
                false => "active",
            }
            .into(),
            ..Default::default()
        };
        self.update_agent(agent, &["status"]).await
    }

    /// Replace an agent's registration (sponsor, scope, models, budgets,
    /// bindings, expiry); `spec` names the agent.
    pub async fn replace_agent(&self, spec: pb::AgentSpec) -> Result<pb::Agent> {
        let agent = pb::Agent {
            name: self.child("agents", &spec.name),
            spec: spec.into(),
            ..Default::default()
        };
        self.update_agent(agent, &["spec"]).await
    }

    async fn update_agent(&self, agent: pb::Agent, paths: &[&str]) -> Result<pb::Agent> {
        let request = pb::UpdateAgentRequest {
            request_id: Self::request_id(),
            agent: agent.into(),
            update_mask: mask(paths).into(),
            etag: String::new(),
        };
        let resp = retry_idempotent(|| {
            let (svc, request) = (self.svc(), request.clone());
            async move { svc.update_agent(request).await }
        })
        .await?
        .into_owned();
        Ok(required("update_agent.agent", resp.agent)?)
    }
}

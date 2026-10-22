//! Agents registered in an organization. Agent names share the
//! instance-wide principal index with users, by kind.

use bytes::Bytes;
use rill_types::identity::{AgentRecord, IdentityChange};
use rill_types::Principal;

use super::store::IdentityStore;
use crate::error::{Error, Result};
use crate::keys::{self, table};
use crate::meta::{KeyRange, Transaction};
use crate::repo::{decode_record, encode_record};

fn agent_key(org: &str, name: &str) -> Bytes {
    keys::identity_pair(table::IDENT_AGENT, org, name)
}

fn principal_key(principal: &Principal) -> Bytes {
    keys::identity_pair(
        table::IDENT_PRINCIPAL,
        principal.kind.as_str(),
        principal.name.as_str(),
    )
}

fn changed(agent: &AgentRecord) -> IdentityChange {
    IdentityChange::AgentChanged {
        org: agent.org.clone(),
        name: agent.principal.name.clone(),
    }
}

impl IdentityStore {
    pub async fn agent(&self, org: &str, name: &str) -> Result<Option<AgentRecord>> {
        match self.meta.get(&agent_key(org, name)).await? {
            Some(bytes) => Ok(Some(decode_record(&bytes)?)),
            None => Ok(None),
        }
    }

    pub async fn agents(&self, org: &str) -> Result<Vec<AgentRecord>> {
        let rows = self
            .meta
            .scan(KeyRange::prefix(keys::identity_row(
                table::IDENT_AGENT,
                org,
            )))
            .await?;
        rows.iter().map(|(_, v)| decode_record(v)).collect()
    }

    /// Register an agent. Fails with a conflict when the name is taken by
    /// another agent anywhere in the instance.
    pub async fn create_agent(&self, agent: &AgentRecord, actor: &Principal) -> Result<()> {
        let key = agent_key(&agent.org, &agent.principal.name);
        let tx = Transaction::new()
            .expect_absent(key.clone())
            .expect_absent(principal_key(&agent.principal))
            .set(key, encode_record(agent)?)
            .set(
                principal_key(&agent.principal),
                Bytes::from(agent.org.clone()),
            );
        let tx = self.logged(tx, changed(agent), actor)?;
        self.meta.transact(tx).await.map_err(|e| match e {
            Error::Conflict(_) => {
                Error::Conflict(format!("an agent named {:?} exists", agent.principal.name))
            }
            other => other,
        })?;
        Ok(())
    }

    /// Replace `before` with `after`; a conflict when it changed since.
    pub async fn update_agent(
        &self,
        before: &AgentRecord,
        after: &AgentRecord,
        actor: &Principal,
    ) -> Result<()> {
        if before.org != after.org || before.principal != after.principal {
            return Err(Error::Invalid(
                "an agent's org and name cannot change".into(),
            ));
        }
        let key = agent_key(&after.org, &after.principal.name);
        let tx = Transaction::new()
            .expect(key.clone(), Some(encode_record(before)?))
            .set(key, encode_record(after)?);
        let tx = self.logged(tx, changed(after), actor)?;
        self.meta.transact(tx).await?;
        Ok(())
    }
}

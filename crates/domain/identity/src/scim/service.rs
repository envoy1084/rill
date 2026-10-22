//! SCIM operations on one organization's users and teams.

use rill_store::identity::IdentityStore;
use rill_types::identity::{AccountStatus, AgentRecord, Sponsor, UserRecord};
use rill_types::{Principal, Timestamp};
use serde_json::{json, Value};

use super::error::ScimError;
use super::filter::Filter;
use super::patch;
use super::resources::{user_resource, UserFields};
use super::LIST_SCHEMA;

/// Paging of a list request (1-based `startIndex`).
#[derive(Debug, Clone, Default)]
pub struct ListQuery {
    pub filter: Option<String>,
    pub start_index: Option<usize>,
    pub count: Option<usize>,
}

/// Most resources returned in one page.
const MAX_PAGE: usize = 200;

pub(super) fn new_id() -> String {
    hex::encode(rand::random::<[u8; 16]>())
}

/// SCIM over one org.
pub struct Scim {
    pub(super) store: IdentityStore,
    pub(super) org: String,
    /// The org's SCIM base URL, for `meta.location`.
    pub(super) base: String,
}

impl Scim {
    pub fn new(store: IdentityStore, org: impl Into<String>, base: impl Into<String>) -> Self {
        Scim {
            store,
            org: org.into(),
            base: base.into(),
        }
    }

    pub(super) fn actor(&self) -> Principal {
        Principal::service("scim")
    }

    pub(super) async fn user_by_id(&self, id: &str) -> Result<UserRecord, ScimError> {
        self.store
            .user_by_scim_id(&self.org, id)
            .await?
            .ok_or_else(|| ScimError::not_found(format!("user {id}")))
    }

    async fn user_body(&self, user: &UserRecord) -> Result<Value, ScimError> {
        let mut groups = Vec::new();
        for team in &user.teams {
            if let Some(t) = self.store.team(&self.org, team).await? {
                groups.push((t.scim_id, t.name));
            }
        }
        Ok(user_resource(user, &groups, &self.base))
    }

    /// `POST /Users`. A user of that name with no SCIM id yet (signed in
    /// before provisioning started) is adopted.
    pub async fn create_user(&self, body: &Value) -> Result<Value, ScimError> {
        let fields = UserFields::parse(body)?;
        let now = Timestamp::now();
        if let Some(existing) = self.store.user(&self.org, &fields.user_name).await? {
            if existing.scim_id.is_some() {
                return Err(ScimError::conflict(format!(
                    "user {} exists",
                    fields.user_name
                )));
            }
            let adopted = UserRecord {
                scim_id: Some(new_id()),
                ..existing.clone()
            };
            let adopted = self.apply_fields(adopted, &fields, now);
            self.store
                .update_user(&existing, &adopted, &self.actor())
                .await?;
            return self.user_body(&adopted).await;
        }
        let user = UserRecord {
            org: self.org.clone(),
            principal: Principal::human(&fields.user_name),
            email: None,
            display_name: None,
            external: Vec::new(),
            status: AccountStatus::Active,
            roles: Vec::new(),
            teams: Vec::new(),
            scim_id: Some(new_id()),
            external_id: None,
            created_at: now,
            updated_at: now,
        };
        let user = self.apply_fields(user, &fields, now);
        self.store.create_user(&user, &self.actor()).await?;
        if !user.is_active() {
            self.deprovision(&user).await?;
        }
        self.user_body(&user).await
    }

    fn apply_fields(
        &self,
        mut user: UserRecord,
        fields: &UserFields,
        now: Timestamp,
    ) -> UserRecord {
        user.email = fields.email.clone();
        user.display_name = fields.display_name.clone();
        user.external_id = fields.external_id.clone();
        user.status = if fields.active {
            AccountStatus::Active
        } else {
            AccountStatus::Suspended
        };
        user.updated_at = now;
        user
    }

    pub async fn get_user(&self, id: &str) -> Result<Value, ScimError> {
        let user = self.user_by_id(id).await?;
        self.user_body(&user).await
    }

    /// `GET /Users`, filtered by `userName`, `externalId` or `id`.
    pub async fn list_users(&self, query: &ListQuery) -> Result<Value, ScimError> {
        let filter = query.filter.as_deref().map(Filter::parse).transpose()?;
        let mut matched = Vec::new();
        for user in self.store.users(&self.org).await? {
            if user.scim_id.is_none() {
                continue;
            }
            let keep = filter.as_ref().is_none_or(|f| {
                f.matches(|attr| match attr {
                    "username" => Some(user.principal.name.clone()),
                    "externalid" => user.external_id.clone(),
                    "id" => user.scim_id.clone(),
                    _ => None,
                })
            });
            if keep {
                matched.push(user);
            }
        }
        let page = page(&matched, query);
        let mut resources = Vec::new();
        for user in page {
            resources.push(self.user_body(user).await?);
        }
        Ok(list(matched.len(), query, resources))
    }

    /// `PUT /Users/{id}`.
    pub async fn replace_user(&self, id: &str, body: &Value) -> Result<Value, ScimError> {
        let fields = UserFields::parse(body)?;
        let before = self.user_by_id(id).await?;
        if fields.user_name != before.principal.name {
            return Err(ScimError::bad_request(
                "mutability",
                "userName cannot change",
            ));
        }
        self.save_user(before, fields).await
    }

    /// `PATCH /Users/{id}`.
    pub async fn patch_user(&self, id: &str, body: &Value) -> Result<Value, ScimError> {
        let before = self.user_by_id(id).await?;
        let mut fields = UserFields {
            user_name: before.principal.name.clone(),
            external_id: before.external_id.clone(),
            display_name: before.display_name.clone(),
            email: before.email.clone(),
            active: before.is_active(),
        };
        patch::patch_user(&mut fields, body)?;
        self.save_user(before, fields).await
    }

    async fn save_user(&self, before: UserRecord, fields: UserFields) -> Result<Value, ScimError> {
        let after = self.apply_fields(before.clone(), &fields, Timestamp::now());
        self.store
            .update_user(&before, &after, &self.actor())
            .await?;
        if before.is_active() && !after.is_active() {
            self.deprovision(&after).await?;
        }
        self.user_body(&after).await
    }

    /// `DELETE /Users/{id}`: the user is suspended, not erased; their
    /// history stays attributed.
    pub async fn delete_user(&self, id: &str) -> Result<(), ScimError> {
        let before = self.user_by_id(id).await?;
        let after = UserRecord {
            status: AccountStatus::Suspended,
            updated_at: Timestamp::now(),
            ..before.clone()
        };
        if before != after {
            self.store
                .update_user(&before, &after, &self.actor())
                .await?;
        }
        self.deprovision(&after).await
    }

    /// A deprovisioned user's sessions end and the agents they sponsor are
    /// suspended until an administrator reassigns them.
    async fn deprovision(&self, user: &UserRecord) -> Result<(), ScimError> {
        let actor = self.actor();
        for session in self.store.live_sessions(Timestamp::now(), 100_000).await? {
            if session.org == self.org && session.principal == user.principal {
                self.store.revoke_session(&session.id, &actor).await?;
            }
        }
        for agent in self.store.agents(&self.org).await? {
            if agent.sponsor == Sponsor::User(user.principal.name.clone())
                && agent.status == AccountStatus::Active
            {
                let after = AgentRecord {
                    status: AccountStatus::Suspended,
                    updated_at: Timestamp::now(),
                    ..agent.clone()
                };
                self.store.update_agent(&agent, &after, &actor).await?;
            }
        }
        tracing::info!(org = %self.org, user = %user.principal, "user deprovisioned");
        Ok(())
    }
}

pub(super) fn page<'a, T>(items: &'a [T], query: &ListQuery) -> &'a [T] {
    let start = query.start_index.unwrap_or(1).max(1) - 1;
    let count = query.count.unwrap_or(MAX_PAGE).min(MAX_PAGE);
    let start = start.min(items.len());
    &items[start..(start + count).min(items.len())]
}

pub(super) fn list(total: usize, query: &ListQuery, resources: Vec<Value>) -> Value {
    json!({
        "schemas": [LIST_SCHEMA],
        "totalResults": total,
        "startIndex": query.start_index.unwrap_or(1).max(1),
        "itemsPerPage": resources.len(),
        "Resources": resources,
    })
}

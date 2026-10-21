//! SCIM groups: teams, whose membership members' `teams` follow.

use rill_types::identity::{TeamRecord, UserRecord};
use rill_types::Timestamp;
use serde_json::Value;

use super::error::ScimError;
use super::filter::Filter;
use super::patch;
use super::resources::group_resource;
use super::service::{list, new_id, page, ListQuery, Scim};

impl Scim {
    async fn team_by_id(&self, id: &str) -> Result<TeamRecord, ScimError> {
        self.store
            .team_by_scim_id(&self.org, id)
            .await?
            .ok_or_else(|| ScimError::not_found(format!("group {id}")))
    }

    async fn group_body(&self, team: &TeamRecord) -> Result<Value, ScimError> {
        let mut members = Vec::new();
        for name in &team.members {
            if let Some(u) = self.store.user(&self.org, name).await? {
                members.push((u.scim_id.unwrap_or_default(), name.clone()));
            }
        }
        Ok(group_resource(team, &members, &self.base))
    }

    /// Member user names for SCIM member ids.
    async fn member_names(&self, ids: &[String]) -> Result<Vec<String>, ScimError> {
        let mut names = Vec::new();
        for id in ids {
            let user = self
                .user_by_id(id)
                .await
                .map_err(|_| ScimError::bad_request("invalidValue", format!("no user {id}")))?;
            if !names.contains(&user.principal.name) {
                names.push(user.principal.name);
            }
        }
        Ok(names)
    }

    async fn member_ids(&self, team: &TeamRecord) -> Result<Vec<String>, ScimError> {
        let mut ids = Vec::new();
        for name in &team.members {
            if let Some(id) = self
                .store
                .user(&self.org, name)
                .await?
                .and_then(|u| u.scim_id)
            {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// Make every affected user's `teams` follow the group.
    async fn sync_members(
        &self,
        before: Option<&TeamRecord>,
        after: Option<&TeamRecord>,
    ) -> Result<(), ScimError> {
        let mut names: Vec<String> = before.map(|t| t.members.clone()).unwrap_or_default();
        for name in after.map(|t| t.members.clone()).unwrap_or_default() {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        for name in names {
            let Some(user) = self.store.user(&self.org, &name).await? else {
                continue;
            };
            let mut teams: Vec<String> = user.teams.clone();
            if let Some(old) = before {
                teams.retain(|t| *t != old.name);
            }
            if let Some(new) = after {
                if new.members.contains(&name) && !teams.contains(&new.name) {
                    teams.push(new.name.clone());
                }
            }
            if teams != user.teams {
                let updated = UserRecord {
                    teams,
                    updated_at: Timestamp::now(),
                    ..user.clone()
                };
                self.store
                    .update_user(&user, &updated, &self.actor())
                    .await?;
            }
        }
        Ok(())
    }

    /// `POST /Groups`.
    pub async fn create_group(&self, body: &Value) -> Result<Value, ScimError> {
        let name = body["displayName"]
            .as_str()
            .filter(|n| !n.is_empty())
            .ok_or_else(|| ScimError::bad_request("invalidValue", "displayName is required"))?;
        let ids: Vec<String> = body["members"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["value"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let now = Timestamp::now();
        let team = TeamRecord {
            org: self.org.clone(),
            name: name.to_string(),
            scim_id: new_id(),
            external_id: body["externalId"].as_str().map(String::from),
            members: self.member_names(&ids).await?,
            created_at: now,
            updated_at: now,
        };
        self.store.create_team(&team, &self.actor()).await?;
        self.sync_members(None, Some(&team)).await?;
        self.group_body(&team).await
    }

    pub async fn get_group(&self, id: &str) -> Result<Value, ScimError> {
        let team = self.team_by_id(id).await?;
        self.group_body(&team).await
    }

    /// `GET /Groups`, filtered by `displayName`, `externalId` or `id`.
    pub async fn list_groups(&self, query: &ListQuery) -> Result<Value, ScimError> {
        let filter = query.filter.as_deref().map(Filter::parse).transpose()?;
        let matched: Vec<TeamRecord> = self
            .store
            .teams(&self.org)
            .await?
            .into_iter()
            .filter(|t| {
                filter.as_ref().is_none_or(|f| {
                    f.matches(|attr| match attr {
                        "displayname" => Some(t.name.clone()),
                        "externalid" => t.external_id.clone(),
                        "id" => Some(t.scim_id.clone()),
                        _ => None,
                    })
                })
            })
            .collect();
        let mut resources = Vec::new();
        for team in page(&matched, query) {
            resources.push(self.group_body(team).await?);
        }
        Ok(list(matched.len(), query, resources))
    }

    /// `PUT /Groups/{id}`.
    pub async fn replace_group(&self, id: &str, body: &Value) -> Result<Value, ScimError> {
        let before = self.team_by_id(id).await?;
        let ids: Vec<String> = body["members"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["value"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let name = body["displayName"]
            .as_str()
            .unwrap_or(&before.name)
            .to_string();
        self.save_group(before, name, ids).await
    }

    /// `PATCH /Groups/{id}`.
    pub async fn patch_group(&self, id: &str, body: &Value) -> Result<Value, ScimError> {
        let before = self.team_by_id(id).await?;
        let mut name = before.name.clone();
        let mut ids = self.member_ids(&before).await?;
        patch::patch_group(&mut name, &mut ids, body)?;
        self.save_group(before, name, ids).await
    }

    async fn save_group(
        &self,
        before: TeamRecord,
        name: String,
        ids: Vec<String>,
    ) -> Result<Value, ScimError> {
        let after = TeamRecord {
            name,
            members: self.member_names(&ids).await?,
            updated_at: Timestamp::now(),
            ..before.clone()
        };
        self.store
            .update_team(&before, &after, &self.actor())
            .await?;
        self.sync_members(Some(&before), Some(&after)).await?;
        self.group_body(&after).await
    }

    /// `DELETE /Groups/{id}`.
    pub async fn delete_group(&self, id: &str) -> Result<(), ScimError> {
        let team = self.team_by_id(id).await?;
        self.store.delete_team(&team, &self.actor()).await?;
        self.sync_members(Some(&team), None).await
    }
}

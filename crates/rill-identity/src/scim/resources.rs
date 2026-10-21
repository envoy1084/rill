//! Users and teams as SCIM resources.

use serde_json::{json, Value};

use super::error::ScimError;
use super::{GROUP_SCHEMA, USER_SCHEMA};
use rill_types::identity::{AccountStatus, TeamRecord, UserRecord};

/// What a provisioning system says about a user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFields {
    pub user_name: String,
    pub external_id: Option<String>,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub active: bool,
}

impl UserFields {
    /// Read a User resource body.
    pub fn parse(body: &Value) -> Result<Self, ScimError> {
        let user_name = body["userName"]
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .ok_or_else(|| ScimError::bad_request("invalidValue", "userName is required"))?;
        if user_name.len() > 128
            || user_name
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
        {
            return Err(ScimError::bad_request(
                "invalidValue",
                format!("{user_name:?} cannot name a user"),
            ));
        }
        let name = &body["name"];
        let display_name = body["displayName"]
            .as_str()
            .or_else(|| name["formatted"].as_str())
            .map(String::from)
            .or_else(|| {
                let given = name["givenName"].as_str().unwrap_or_default();
                let family = name["familyName"].as_str().unwrap_or_default();
                let full = format!("{given} {family}").trim().to_string();
                (!full.is_empty()).then_some(full)
            });
        Ok(UserFields {
            user_name: user_name.to_string(),
            external_id: body["externalId"].as_str().map(String::from),
            display_name,
            email: primary_email(&body["emails"]),
            active: active_value(&body["active"]).unwrap_or(true),
        })
    }
}

/// The primary email of an `emails` array, or its first.
pub fn primary_email(emails: &Value) -> Option<String> {
    let list = emails.as_array()?;
    list.iter()
        .find(|e| e["primary"].as_bool() == Some(true))
        .or_else(|| list.first())
        .and_then(|e| e["value"].as_str())
        .map(String::from)
}

/// `active` as a boolean; Entra ID sends it as a string.
pub fn active_value(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::String(s) if s.eq_ignore_ascii_case("true") => Some(true),
        Value::String(s) if s.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}

fn meta(kind: &str, base: &str, id: &str, created: i64, updated: i64) -> Value {
    let ts = |ms| rill_types::Timestamp::from_millis(ms).to_rfc3339();
    json!({
        "resourceType": kind,
        "created": ts(created),
        "lastModified": ts(updated),
        "location": format!("{base}/{kind}s/{id}"),
    })
}

/// A user as a SCIM User; `groups` are its teams' `(id, name)`.
pub fn user_resource(user: &UserRecord, groups: &[(String, String)], base: &str) -> Value {
    let id = user.scim_id.clone().unwrap_or_default();
    let mut body = json!({
        "schemas": [USER_SCHEMA],
        "id": id,
        "userName": user.principal.name,
        "active": user.status == AccountStatus::Active,
        "groups": groups.iter().map(|(gid, name)| json!({"value": gid, "display": name})).collect::<Vec<_>>(),
        "meta": meta("User", base, &id, user.created_at.millis(), user.updated_at.millis()),
    });
    if let Some(e) = &user.external_id {
        body["externalId"] = json!(e);
    }
    if let Some(d) = &user.display_name {
        body["displayName"] = json!(d);
        body["name"] = json!({ "formatted": d });
    }
    if let Some(e) = &user.email {
        body["emails"] = json!([{ "value": e, "primary": true, "type": "work" }]);
    }
    body
}

/// A team as a SCIM Group; `members` are `(user id, user name)`.
pub fn group_resource(team: &TeamRecord, members: &[(String, String)], base: &str) -> Value {
    let mut body = json!({
        "schemas": [GROUP_SCHEMA],
        "id": team.scim_id,
        "displayName": team.name,
        "members": members.iter().map(|(id, name)| json!({"value": id, "display": name})).collect::<Vec<_>>(),
        "meta": meta("Group", base, &team.scim_id, team.created_at.millis(), team.updated_at.millis()),
    });
    if let Some(e) = &team.external_id {
        body["externalId"] = json!(e);
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn okta_and_entra_user_bodies_parse() {
        let okta = json!({
            "schemas": [USER_SCHEMA], "userName": "ann@example.com",
            "name": {"givenName": "Ann", "familyName": "Lee"},
            "emails": [{"primary": true, "value": "ann@example.com", "type": "work"}],
            "active": true,
        });
        let fields = UserFields::parse(&okta).unwrap();
        assert_eq!(fields.user_name, "ann@example.com");
        assert_eq!(fields.display_name.as_deref(), Some("Ann Lee"));
        assert_eq!(fields.email.as_deref(), Some("ann@example.com"));
        assert!(fields.active);

        let entra = json!({
            "userName": "bob@example.com", "externalId": "0a1b", "active": "False",
            "displayName": "Bob",
        });
        let fields = UserFields::parse(&entra).unwrap();
        assert_eq!(fields.external_id.as_deref(), Some("0a1b"));
        assert!(!fields.active);
        assert!(UserFields::parse(&json!({"userName": "has space"})).is_err());
        assert!(UserFields::parse(&json!({})).is_err());
    }
}

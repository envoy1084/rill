//! PatchOp requests (RFC 7644 §3.5.2) in the shapes Okta and Entra ID
//! send them.

use serde_json::Value;

use super::error::ScimError;
use super::filter::Filter;
use super::resources::{active_value, primary_email, UserFields};

fn operations(body: &Value) -> Result<&Vec<Value>, ScimError> {
    body["Operations"]
        .as_array()
        .ok_or_else(|| ScimError::bad_request("invalidSyntax", "Operations is required"))
}

fn op_name(op: &Value) -> Result<String, ScimError> {
    op["op"]
        .as_str()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| ScimError::bad_request("invalidSyntax", "an operation needs op"))
}

/// Apply a PatchOp body to `user`.
pub fn patch_user(user: &mut UserFields, body: &Value) -> Result<(), ScimError> {
    for op in operations(body)? {
        let name = op_name(op)?;
        if !matches!(name.as_str(), "add" | "replace" | "remove") {
            return Err(ScimError::bad_request(
                "invalidSyntax",
                format!("unknown op {name:?}"),
            ));
        }
        let remove = name == "remove";
        match op["path"].as_str() {
            None => {
                let Some(fields) = op["value"].as_object() else {
                    return Err(ScimError::bad_request(
                        "noTarget",
                        "a path or an object value is required",
                    ));
                };
                for (path, value) in fields {
                    set_user_attribute(user, path, value, remove)?;
                }
            }
            Some(path) => set_user_attribute(user, path, &op["value"], remove)?,
        }
    }
    Ok(())
}

fn set_user_attribute(
    user: &mut UserFields,
    path: &str,
    value: &Value,
    remove: bool,
) -> Result<(), ScimError> {
    let attribute = path
        .split(['[', '.'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    match attribute.as_str() {
        "active" => {
            user.active = if remove {
                false
            } else {
                active_value(value).ok_or_else(|| {
                    ScimError::bad_request("invalidValue", "active must be a boolean")
                })?
            }
        }
        "displayname" => {
            user.display_name = (!remove)
                .then(|| value.as_str().map(String::from))
                .flatten()
        }
        "name" => {
            if let Some(formatted) = value["formatted"].as_str().or_else(|| value.as_str()) {
                user.display_name = Some(formatted.to_string());
            }
        }
        "externalid" => {
            user.external_id = (!remove)
                .then(|| value.as_str().map(String::from))
                .flatten()
        }
        "emails" => {
            user.email = if remove {
                None
            } else if let Some(text) = value.as_str() {
                Some(text.to_string())
            } else {
                primary_email(value)
            }
        }
        "username" if value.as_str() != Some(user.user_name.as_str()) => {
            return Err(ScimError::bad_request(
                "mutability",
                "userName cannot change",
            ));
        }
        // Attributes Rill does not keep (title, locale, …) are accepted and
        // dropped, as provisioning systems send them unasked.
        _ => {}
    }
    Ok(())
}

/// Apply a PatchOp body to a group's name and member ids.
pub fn patch_group(
    name: &mut String,
    members: &mut Vec<String>,
    body: &Value,
) -> Result<(), ScimError> {
    let ids = |value: &Value| -> Vec<String> {
        value
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|m| m["value"].as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    for op in operations(body)? {
        let kind = op_name(op)?;
        let path = op["path"].as_str().unwrap_or_default();
        let lower = path.to_ascii_lowercase();
        if lower.is_empty() {
            if let Some(display) = op["value"]["displayName"].as_str() {
                *name = display.to_string();
            }
            if op["value"]["members"].is_array() {
                apply_members(&kind, members, ids(&op["value"]["members"]));
            }
        } else if lower == "displayname" {
            if let Some(display) = op["value"].as_str() {
                *name = display.to_string();
            }
        } else if lower == "members" {
            apply_members(&kind, members, ids(&op["value"]));
        } else if lower.starts_with("members[") && lower.ends_with(']') {
            // `members[value eq "id"]`, as Entra ID removes a member.
            let filter = Filter::parse(&path["members[".len()..path.len() - 1])?;
            let id = filter
                .get("value")
                .ok_or_else(|| ScimError::bad_request("invalidPath", path.to_string()))?
                .to_string();
            apply_members(&kind, members, vec![id]);
        } else {
            return Err(ScimError::bad_request("invalidPath", path.to_string()));
        }
    }
    Ok(())
}

fn apply_members(kind: &str, members: &mut Vec<String>, ids: Vec<String>) {
    match kind {
        "add" => {
            for id in ids {
                if !members.contains(&id) {
                    members.push(id);
                }
            }
        }
        "remove" => members.retain(|m| !ids.contains(m)),
        _ => *members = ids,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn fields() -> UserFields {
        UserFields {
            user_name: "ann".into(),
            external_id: None,
            display_name: None,
            email: None,
            active: true,
        }
    }

    #[test]
    fn okta_and_entra_deactivations_apply() {
        let mut user = fields();
        patch_user(
            &mut user,
            &json!({"Operations": [{"op": "replace", "value": {"active": false}}]}),
        )
        .unwrap();
        assert!(!user.active);
        let mut user = fields();
        patch_user(
            &mut user,
            &json!({"Operations": [{"op": "Replace", "path": "active", "value": "False"}]}),
        )
        .unwrap();
        assert!(!user.active);
        let mut user = fields();
        patch_user(
            &mut user,
            &json!({"Operations": [
                {"op": "Add", "path": "emails[type eq \"work\"].value", "value": "ann@x.com"},
                {"op": "Replace", "path": "displayName", "value": "Ann"},
                {"op": "Add", "path": "title", "value": "eng"},
            ]}),
        )
        .unwrap();
        assert_eq!(user.email.as_deref(), Some("ann@x.com"));
        assert_eq!(user.display_name.as_deref(), Some("Ann"));
        assert!(patch_user(
            &mut user,
            &json!({"Operations": [{"op": "replace", "path": "userName", "value": "eve"}]})
        )
        .is_err());
    }

    #[test]
    fn group_membership_patches_apply_in_order() {
        let (mut name, mut members) = ("devs".to_string(), vec!["u1".to_string()]);
        patch_group(
            &mut name,
            &mut members,
            &json!({"Operations": [
                {"op": "add", "path": "members", "value": [{"value": "u2"}, {"value": "u1"}]},
                {"op": "Remove", "path": "members[value eq \"u1\"]"},
                {"op": "replace", "value": {"id": "g", "displayName": "developers"}},
            ]}),
        )
        .unwrap();
        assert_eq!(members, ["u2"]);
        assert_eq!(name, "developers");
        patch_group(&mut name, &mut members, &json!({"Operations": [{"op": "remove", "path": "members", "value": [{"value": "u2"}]}]})).unwrap();
        assert!(members.is_empty());
    }
}

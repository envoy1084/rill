//! The resources a provisioning system reads before provisioning.

use axum::http::StatusCode;
use axum::response::Response;
use rill_identity::scim::{GROUP_SCHEMA, USER_SCHEMA};
use serde_json::json;

use super::scim_json;

const LIST: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";

pub async fn service_provider_config() -> Response {
    scim_json(
        StatusCode::OK,
        json!({
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"],
            "patch": { "supported": true },
            "bulk": { "supported": false, "maxOperations": 0, "maxPayloadSize": 0 },
            "filter": { "supported": true, "maxResults": 200 },
            "changePassword": { "supported": false },
            "sort": { "supported": false },
            "etag": { "supported": false },
            "authenticationSchemes": [{
                "type": "oauthbearertoken",
                "name": "Bearer token",
                "description": "A SCIM credential of the organization",
                "primary": true,
            }],
        }),
    )
}

pub async fn resource_types() -> Response {
    scim_json(
        StatusCode::OK,
        json!({
            "schemas": [LIST],
            "totalResults": 2,
            "Resources": [
                { "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ResourceType"],
                  "id": "User", "name": "User", "endpoint": "/Users", "schema": USER_SCHEMA },
                { "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ResourceType"],
                  "id": "Group", "name": "Group", "endpoint": "/Groups", "schema": GROUP_SCHEMA },
            ],
        }),
    )
}

pub async fn schemas() -> Response {
    let attribute = |name: &str, kind: &str, required: bool, multi: bool| {
        json!({ "name": name, "type": kind, "required": required, "multiValued": multi,
                "mutability": "readWrite", "returned": "default" })
    };
    scim_json(
        StatusCode::OK,
        json!({
            "schemas": [LIST],
            "totalResults": 2,
            "Resources": [
                { "id": USER_SCHEMA, "name": "User", "attributes": [
                    attribute("userName", "string", true, false),
                    attribute("displayName", "string", false, false),
                    attribute("emails", "complex", false, true),
                    attribute("active", "boolean", false, false),
                    attribute("externalId", "string", false, false),
                ]},
                { "id": GROUP_SCHEMA, "name": "Group", "attributes": [
                    attribute("displayName", "string", true, false),
                    attribute("members", "complex", false, true),
                ]},
            ],
        }),
    )
}

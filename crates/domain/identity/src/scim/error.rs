//! SCIM error responses (RFC 7644 §3.12).

use serde_json::{json, Value};

/// A SCIM error: an HTTP status, an optional `scimType` and a detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScimError {
    pub status: u16,
    pub scim_type: Option<&'static str>,
    pub detail: String,
}

impl ScimError {
    pub fn bad_request(scim_type: &'static str, detail: impl Into<String>) -> Self {
        ScimError {
            status: 400,
            scim_type: Some(scim_type),
            detail: detail.into(),
        }
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        ScimError {
            status: 404,
            scim_type: None,
            detail: detail.into(),
        }
    }

    pub fn conflict(detail: impl Into<String>) -> Self {
        ScimError {
            status: 409,
            scim_type: Some("uniqueness"),
            detail: detail.into(),
        }
    }

    pub fn internal(detail: impl std::fmt::Display) -> Self {
        ScimError {
            status: 500,
            scim_type: None,
            detail: detail.to_string(),
        }
    }

    /// The response body.
    pub fn body(&self) -> Value {
        let mut body = json!({
            "schemas": [super::ERROR_SCHEMA],
            "status": self.status.to_string(),
            "detail": self.detail,
        });
        if let Some(t) = self.scim_type {
            body["scimType"] = json!(t);
        }
        body
    }
}

impl From<rill_store::Error> for ScimError {
    fn from(e: rill_store::Error) -> Self {
        match e {
            rill_store::Error::Conflict(m) => ScimError::conflict(m),
            other => ScimError::internal(other),
        }
    }
}

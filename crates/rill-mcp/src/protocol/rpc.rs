//! JSON-RPC 2.0 request, response and error shapes.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Invalid JSON was received.
pub const PARSE_ERROR: i64 = -32700;
/// The JSON was not a valid request object.
pub const INVALID_REQUEST: i64 = -32600;
/// The method does not exist.
pub const METHOD_NOT_FOUND: i64 = -32601;
/// The params are invalid (MCP also uses this for an unknown tool).
pub const INVALID_PARAMS: i64 = -32602;
/// The server failed.
pub const INTERNAL_ERROR: i64 = -32603;

/// A request id: a number or a string. Notifications have none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    /// Numeric id.
    Number(i64),
    /// String id.
    String(String),
}

/// An incoming request or notification.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    /// Must be `"2.0"`.
    #[serde(default)]
    pub jsonrpc: String,
    /// Absent for notifications.
    #[serde(default)]
    pub id: Option<Id>,
    /// Method name.
    pub method: String,
    /// Parameters, if any.
    #[serde(default)]
    pub params: Value,
}

impl Request {
    /// True for a notification, which gets no response.
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    /// Error code.
    pub code: i64,
    /// Human-readable message.
    pub message: String,
    /// Extra detail.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub data: Value,
}

impl RpcError {
    /// Build an error without data.
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        RpcError {
            code,
            message: message.into(),
            data: Value::Null,
        }
    }

    /// Attach detail.
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }
}

/// An outgoing response: exactly one of `result` and `error` is set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Always `"2.0"`.
    pub jsonrpc: String,
    /// The request id; `null` when the request could not be parsed.
    pub id: Option<Id>,
    /// Success payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Failure payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// A success response.
    pub fn success(id: Option<Id>, result: Value) -> Self {
        Response {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// A failure response.
    pub fn failure(id: Option<Id>, error: RpcError) -> Self {
        Response {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_request_and_notification() {
        let r: Request =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#).unwrap();
        assert_eq!(r.id, Some(Id::Number(1)));
        assert!(!r.is_notification());
        let n: Request =
            serde_json::from_str(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#)
                .unwrap();
        assert!(n.is_notification());
    }

    #[test]
    fn response_serializes_one_side() {
        let ok = serde_json::to_value(Response::success(Some(Id::String("a".into())), Value::Null))
            .unwrap();
        assert!(ok.get("error").is_none());
        let err = serde_json::to_value(Response::failure(None, RpcError::new(PARSE_ERROR, "bad")))
            .unwrap();
        assert!(err.get("result").is_none());
        assert_eq!(err["id"], Value::Null);
        assert_eq!(err["error"]["code"], PARSE_ERROR);
    }
}

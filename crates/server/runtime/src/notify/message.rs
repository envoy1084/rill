//! What a notification says, for people and for webhooks.

use rill_types::Event;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A message for a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub title: String,
    pub text: String,
}

impl Message {
    pub fn new(title: impl Into<String>, text: impl Into<String>) -> Self {
        Message {
            title: title.into(),
            text: text.into(),
        }
    }

    /// One line for chat.
    pub fn line(&self) -> String {
        if self.text.is_empty() {
            self.title.clone()
        } else {
            format!("*{}*\n{}", self.title, self.text)
        }
    }
}

/// A webhook's body for `event` in `repo`.
pub fn event_body(repo: &str, id: &str, event: &Event) -> Value {
    json!({
        "id": id,
        "type": event.kind.name(),
        "repo": repo,
        "timestamp": event.at.to_rfc3339(),
        "actor": event.actor.to_string(),
        "data": serde_json::to_value(&event.kind).unwrap_or(Value::Null),
    })
}

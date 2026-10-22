//! A tool's advertised name, description and argument schema.

use serde_json::Value;

/// A tool as advertised by `tools/list`.
#[derive(Debug, Clone)]
pub struct ToolDef {
    /// Tool name, `rill_*`.
    pub name: &'static str,
    /// What the tool does, for the model.
    pub description: &'static str,
    /// JSON schema of the `arguments` object.
    pub input_schema: Value,
}

impl ToolDef {
    /// The `tools/list` entry.
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "name": self.name,
            "description": self.description,
            "inputSchema": self.input_schema,
        })
    }
}

//! Small builders for the JSON schemas tools advertise.

use serde_json::{json, Map, Value};

/// A property list under construction.
#[derive(Default)]
pub struct Props {
    props: Map<String, Value>,
    required: Vec<String>,
}

impl Props {
    /// Start with the `repo` property every repository-scoped tool takes.
    pub fn repo() -> Self {
        Props::default().opt(
            "repo",
            string("Repository name or id; defaults to RILL_REPO"),
        )
    }

    /// A required property.
    pub fn req(mut self, name: &str, schema: Value) -> Self {
        self.props.insert(name.to_string(), schema);
        self.required.push(name.to_string());
        self
    }

    /// An optional property.
    pub fn opt(mut self, name: &str, schema: Value) -> Self {
        self.props.insert(name.to_string(), schema);
        self
    }

    /// The finished object schema.
    pub fn build(self) -> Value {
        json!({
            "type": "object",
            "properties": self.props,
            "required": self.required,
            "additionalProperties": false,
        })
    }
}

/// A string property.
pub fn string(desc: &str) -> Value {
    json!({ "type": "string", "description": desc })
}

/// A string property restricted to `values`.
pub fn string_enum(desc: &str, values: &[&str]) -> Value {
    json!({ "type": "string", "description": desc, "enum": values })
}

/// An integer property.
pub fn integer(desc: &str) -> Value {
    json!({ "type": "integer", "description": desc })
}

/// A boolean property.
pub fn boolean(desc: &str) -> Value {
    json!({ "type": "boolean", "description": desc })
}

/// An array of strings.
pub fn string_array(desc: &str) -> Value {
    json!({ "type": "array", "description": desc, "items": { "type": "string" } })
}

/// A root spec: `trunk`, `trunk@N`, a version hash or a ref name.
pub fn root(desc: &str) -> Value {
    string(&format!(
        "{desc}: `trunk`, `trunk@<seq>`, a 64-hex version hash or a ref name"
    ))
}

/// The provenance fields a writing tool accepts.
pub fn provenance_props(p: Props) -> Props {
    p.opt(
        "model",
        string("Model that produced the edit, recorded in provenance"),
    )
    .opt(
        "session",
        string("Agent session id, recorded in provenance"),
    )
    .opt(
        "prompt_ref",
        string("Reference to the prompt or task, recorded in provenance"),
    )
}

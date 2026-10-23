//! The tools a Claude reviewer or resolver must answer through, and the
//! parsing of its answers. Forcing a tool keeps the answer structured;
//! the server still checks it.

use rill_sdk::{parse_category, ReviewFinding};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{Error, Result};

/// The review tool's name.
pub const REVIEW_TOOL: &str = "report_review";
/// The resolve tool's name.
pub const RESOLVE_TOOL: &str = "report_resolution";

/// The review tool.
pub fn review_tool() -> Value {
    json!({
        "name": REVIEW_TOOL,
        "description": "Report the review of the change: findings and a vote.",
        "input_schema": {
            "type": "object",
            "properties": {
                "findings": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "A path from the diff; empty for the whole change." },
                            "start_line": { "type": "integer", "minimum": 0, "description": "First line in the new version; 0 for the whole file." },
                            "end_line": { "type": "integer", "minimum": 0 },
                            "category": { "type": "string", "enum": ["bug", "security", "test_gap", "style", "suggestion", "question"] },
                            "confidence": { "type": "integer", "minimum": 0, "maximum": 100 },
                            "body": { "type": "string" }
                        },
                        "required": ["path", "category", "confidence", "body"]
                    }
                },
                "vote": { "type": "integer", "enum": [-1, 1], "description": "-1 when a finding should block the change, else 1." },
                "summary": { "type": "string" }
            },
            "required": ["findings", "vote", "summary"]
        }
    })
}

/// The resolve tool.
pub fn resolve_tool() -> Value {
    json!({
        "name": RESOLVE_TOOL,
        "description": "Report the resolved content of every conflicted file, or decline.",
        "input_schema": {
            "type": "object",
            "properties": {
                "decline": { "type": "boolean", "description": "True when a human should resolve it." },
                "files": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "content": { "type": ["string", "null"], "description": "The resolved text; null deletes the file." }
                        },
                        "required": ["path", "content"]
                    }
                },
                "rationale": { "type": "string" }
            },
            "required": ["decline", "files", "rationale"]
        }
    })
}

#[derive(Debug, Deserialize)]
struct RawFinding {
    #[serde(default)]
    path: String,
    #[serde(default)]
    start_line: u32,
    #[serde(default)]
    end_line: u32,
    category: String,
    confidence: u32,
    body: String,
}

/// A review tool call's input.
#[derive(Debug, Deserialize)]
pub struct ReviewAnswer {
    findings: Vec<RawFinding>,
    pub vote: i64,
    #[serde(default)]
    pub summary: String,
}

impl ReviewAnswer {
    /// The findings, with unknown categories dropped and ranges made sane.
    pub fn findings(&self) -> Vec<ReviewFinding> {
        self.findings
            .iter()
            .filter_map(|f| {
                let category = parse_category(&f.category)?;
                let lines = match (f.start_line, f.end_line) {
                    (0, _) => None,
                    (s, e) => Some((s, e.max(s))),
                };
                Some(ReviewFinding {
                    path: (!f.path.is_empty()).then(|| f.path.clone()),
                    lines: lines.filter(|_| !f.path.is_empty()),
                    category,
                    confidence: f.confidence.min(100) as u8,
                    body: f.body.clone(),
                })
            })
            .collect()
    }
}

/// A resolve tool call's input.
#[derive(Debug, Deserialize)]
pub struct ResolveAnswer {
    pub decline: bool,
    pub files: Vec<ResolvedFile>,
    #[serde(default)]
    pub rationale: String,
}

/// One resolved file.
#[derive(Debug, Deserialize)]
pub struct ResolvedFile {
    pub path: String,
    pub content: Option<String>,
}

/// The input of the `tool` call in a Messages API response.
pub fn tool_input<T: for<'de> Deserialize<'de>>(response: &Value, tool: &str) -> Result<T> {
    let call = response["content"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["type"] == "tool_use" && c["name"] == tool)
        .ok_or_else(|| Error::Backend(format!("the answer has no {tool} call")))?;
    serde_json::from_value(call["input"].clone())
        .map_err(|e| Error::Backend(format!("{tool} input: {e}")))
}

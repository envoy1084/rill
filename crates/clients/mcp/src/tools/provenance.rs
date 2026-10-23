//! What writes record about who made them, and file kinds as agents name
//! them.

use rill_types::{FileKind, Provenance};

use super::args::Args;
use crate::error::{ToolError, ToolResult};

/// Tool name recorded in every change's provenance.
pub const TOOL_NAME: &str = "rill-mcp";

/// Provenance for a write: this tool plus the model, session and prompt
/// the agent passed.
pub fn provenance(args: &Args) -> ToolResult<Provenance> {
    Ok(Provenance {
        tool: Some(TOOL_NAME.into()),
        model: args.opt_string("model")?,
        session: args.opt_string("session")?,
        prompt_ref: args.opt_string("prompt_ref")?,
        ..Provenance::default()
    })
}

/// The `kind` of a file argument, regular by default.
pub fn file_kind(args: &Args) -> ToolResult<FileKind> {
    Ok(match args.opt_str("kind") {
        None | Some("regular") => FileKind::Regular,
        Some("executable") => FileKind::Executable,
        Some("symlink") => FileKind::Symlink,
        Some(other) => return Err(ToolError::invalid(format!("unknown file kind {other:?}"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provenance_names_the_tool_and_the_agent() {
        let args = Args::new(json!({ "model": "m", "session": "s" })).unwrap();
        let p = provenance(&args).unwrap();
        assert_eq!(p.tool.as_deref(), Some(TOOL_NAME));
        assert_eq!(p.model.as_deref(), Some("m"));
        assert_eq!(p.session.as_deref(), Some("s"));
    }
}

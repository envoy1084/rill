//! `rill_symbols`: where a symbol is defined, where it is used, or which
//! symbols a file (or a name search) has. Precise where a SCIP index was
//! uploaded, else from extracted definitions and whole-word text.

use rill_sdk::ReferenceTarget;
use serde_json::{json, Value};

use super::args::Args;
use super::budget::{self, Budget, Cursor};
use super::root::read_root;
use super::schema::{integer, root, string, string_array, string_enum, Props};
use super::{ToolContext, ToolDef};
use crate::error::{ToolError, ToolResult};
use crate::json;

/// References fetched per call; the budget decides how many are returned.
const MAX_REFERENCES: u32 = 1_000;

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_symbols",
        description: "Code navigation. `find: definitions` of `name`; `find: references` to `name` or to the symbol at `path`+`line`+`column` (precise when the repository uploaded a SCIP index, else whole-word matches); `find: symbols` of the file at `path`, or whose names contain `name`. Uses the session's change by default, else trunk.",
        input_schema: budget::props(
            Props::repo()
                .req("find", string_enum("What to find", &["definitions", "references", "symbols"]))
                .opt("name", string("Symbol name (exact for definitions and references, a substring for symbols)"))
                .opt("path", string("A file: its symbols, or the position's file"))
                .opt("line", integer("One-based line of the position"))
                .opt("column", integer("Zero-based column of the position"))
                .opt("kinds", string_array("Only these kinds (`function`, `class`, `struct`, …)"))
                .opt("at", root("Root to look at"))
                .change(),
        )
        .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let root = read_root(ctx, &repo, args).await?;
    let name = args.opt_string("name")?;
    let (found, precise): (Vec<Value>, Option<bool>) = match args.str("find")? {
        "definitions" => {
            let name = name.ok_or_else(|| ToolError::invalid("definitions need `name`"))?;
            let defs = repo.find_definitions(&root, &name).await?;
            (defs.iter().map(json::symbol_location).collect(), None)
        }
        "references" => {
            let target = match (args.opt_str("path"), name) {
                (Some(_), _) => ReferenceTarget::Position {
                    path: args.path("path")?,
                    line: args.limit("line", 0)?,
                    column: args.limit("column", 0)?,
                },
                (None, Some(name)) => ReferenceTarget::Name(name),
                (None, None) => {
                    return Err(ToolError::invalid(
                        "references need `name` or `path`+`line`+`column`",
                    ))
                }
            };
            let refs = repo.find_references(&root, &target, MAX_REFERENCES).await?;
            (
                refs.references.iter().map(json::symbol_location).collect(),
                Some(refs.precise),
            )
        }
        "symbols" => {
            let kinds = args.opt_str_list("kinds")?;
            let list = match (args.opt_str("path"), name) {
                (Some(_), _) => {
                    let path = args.path("path")?;
                    repo.file_symbols(&root, path.as_path())
                        .await?
                        .into_iter()
                        .filter(|s| {
                            kinds.is_empty()
                                || kinds.iter().any(|k| k.eq_ignore_ascii_case(&s.kind))
                        })
                        .collect()
                }
                (None, Some(q)) => repo.search_symbols(&root, &q, &kinds, 0).await?,
                (None, None) => return Err(ToolError::invalid("symbols need `path` or `name`")),
            };
            (list.iter().map(json::symbol_location).collect(), None)
        }
        other => return Err(ToolError::invalid(format!("unknown `find`: {other}"))),
    };
    let mut budget = Budget::from_args(args)?;
    let (shown, next) = budget::take(&found, Cursor::from_args(args)?.item, &mut budget);
    let mut out = json!({
        "root": root.to_string(),
        "total": found.len(),
        "results": shown,
    });
    if let Some(precise) = precise {
        out["precise"] = json!(precise);
    }
    budget::finish(&mut out, next);
    Ok(out)
}

//! `rill help [topic]`: guides on how to work with Rill, or a command's
//! help (`rill help land cancel`).

use clap::{Args, CommandFactory};

use crate::cli::Cli;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{Out, Role};
use crate::Result;

/// The guides, by topic.
pub const GUIDES: &[(&str, &str, &str)] = &[
    (
        "getting-started",
        "sign in, clone, change, review, land",
        include_str!("../../guides/getting-started.md"),
    ),
    (
        "checkouts",
        "eager, lazy and snapshot checkouts, the daemon",
        include_str!("../../guides/checkouts.md"),
    ),
    (
        "stacks",
        "chains of changes reviewed and landed together",
        include_str!("../../guides/stacks.md"),
    ),
    (
        "conflicts",
        "why they happen and how to resolve them",
        include_str!("../../guides/conflicts.md"),
    ),
    (
        "agents",
        "agent principals, budgets, stopping them",
        include_str!("../../guides/agents.md"),
    ),
    (
        "config",
        "colours, the pager, diffs, aliases",
        include_str!("../../guides/config.md"),
    ),
];

/// `rill help`.
#[derive(Args, Debug)]
pub struct HelpArgs {
    /// A guide (`rill help` lists them) or a command (`land cancel`).
    pub topic: Vec<String>,
}

/// Print a guide: headings and code styled, the rest as written.
fn guide(out: &mut Out<'_>, text: &str) -> std::io::Result<()> {
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("# ") {
            let painted = out.paint(Role::Header, &h.to_uppercase());
            out.line(painted)?;
        } else if let Some(h) = line.strip_prefix("## ") {
            let painted = out.paint(Role::Header, h);
            out.line(painted)?;
        } else if line.starts_with("    ") {
            let painted = out.paint(Role::Id, line);
            out.line(painted)?;
        } else {
            out.line(code_spans(out, line))?;
        }
    }
    Ok(())
}

/// `code` in backticks styled.
fn code_spans(out: &Out<'_>, line: &str) -> String {
    let mut result = String::new();
    for (i, part) in line.split('`').enumerate() {
        if i % 2 == 1 {
            result.push_str(&out.paint(Role::Id, part));
        } else {
            result.push_str(part);
        }
    }
    result
}

/// Run `rill help`.
pub async fn run(ctx: &mut Ctx<'_>, args: HelpArgs) -> Result<()> {
    let topic = args.topic.join(" ");
    if let Some((name, _, text)) = GUIDES.iter().find(|(n, _, _)| *n == topic) {
        return ctx.emit(serde_json::json!({ "topic": name, "text": text }), |out| {
            guide(out, text)
        });
    }
    let mut command = Cli::command();
    command.build();
    let mut cmd = &command;
    for word in &args.topic {
        cmd = cmd.find_subcommand(word).ok_or_else(|| {
            CliError::usage(format!(
                "no guide or command `{topic}`; guides: {}",
                GUIDES
                    .iter()
                    .map(|(n, _, _)| *n)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
    }
    let mut cmd = cmd.clone();
    let color = ctx.out().color();
    let help = if color {
        cmd.render_long_help().ansi().to_string()
    } else {
        cmd.render_long_help().to_string()
    };
    let at_root = args.topic.is_empty();
    ctx.emit(
        serde_json::json!({
            "topic": topic,
            "guides": GUIDES.iter().map(|(n, d, _)| serde_json::json!({ "name": n, "about": d })).collect::<Vec<_>>(),
        }),
        |out| {
            out.raw(help.as_bytes())?;
            if at_root {
                out.blank()?;
                let painted = out.paint(Role::Header, "Guides (rill help <topic>):");
                out.line(painted)?;
                for (name, about, _) in GUIDES {
                    out.line(format!("  {:<16} {about}", out.paint(Role::Id, name)))?;
                }
            }
            Ok(())
        },
    )
}

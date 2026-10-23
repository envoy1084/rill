//! Asking for a message in the user's editor, like `git commit` without
//! `-m`: `$RILL_EDITOR`, `$VISUAL`, `$EDITOR`, else `vi`. Lines starting
//! with `#` are dropped; an empty message aborts.

use std::io::Write;

use crate::error::CliError;
use crate::Result;

/// The editor command.
fn editor() -> String {
    ["RILL_EDITOR", "VISUAL", "EDITOR"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|e| !e.trim().is_empty()))
        .unwrap_or_else(|| "vi".into())
}

/// The message kept from an edited file: comments dropped, surrounding
/// blank lines trimmed.
pub fn clean(text: &str) -> String {
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .map(str::trim_end)
        .collect();
    kept.join("\n").trim().to_string()
}

/// Open the editor on `template` and return the message written.
pub fn edit_message(template: &str) -> Result<String> {
    let mut file = tempfile::Builder::new()
        .prefix("RILL_MESSAGE")
        .suffix(".txt")
        .tempfile()?;
    file.write_all(template.as_bytes())?;
    file.flush()?;
    let words = shlex::split(&editor())
        .ok_or_else(|| CliError::usage("the editor command does not parse"))?;
    let (program, args) = words
        .split_first()
        .ok_or_else(|| CliError::usage("no editor set: use -m or set $EDITOR"))?;
    let status = std::process::Command::new(program)
        .args(args)
        .arg(file.path())
        .status()
        .map_err(|e| CliError::usage(format!("cannot run the editor `{program}`: {e}")))?;
    if !status.success() {
        return Err(CliError::usage("the editor failed; nothing recorded"));
    }
    let message = clean(&std::fs::read_to_string(file.path())?);
    if message.is_empty() {
        return Err(CliError::usage("empty message; nothing recorded"));
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_surrounding_blank_lines_are_dropped() {
        let text = "\n\nfix parser  \n\nmore detail\n# Changes:\n#   modified: a\n";
        assert_eq!(clean(text), "fix parser\n\nmore detail");
        assert_eq!(clean("# only comments\n"), "");
    }
}

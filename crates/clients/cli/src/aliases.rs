//! `[alias]` expansion, like git: the first command word, when it names
//! an alias and no built-in command, is replaced by the alias's words.

use std::collections::BTreeMap;
use std::ffi::OsString;

/// Global options that take a value, so their value is not mistaken for
/// the command word.
const VALUED: &[&str] = &[
    "--endpoint",
    "--token",
    "--repo",
    "--json-schema",
    "-C",
    "--cwd",
    "--daemon-dir",
    "--color",
];

/// `args` (program name first) with an alias expanded. Built-in commands
/// win over aliases of the same name; aliases do not expand recursively.
pub fn expand(
    args: Vec<OsString>,
    aliases: &BTreeMap<String, String>,
    builtins: &[String],
) -> Vec<OsString> {
    if aliases.is_empty() {
        return args;
    }
    let mut i = 1;
    while i < args.len() {
        let word = args[i].to_string_lossy();
        if VALUED.contains(&word.as_ref()) {
            i += 2;
            continue;
        }
        if word.starts_with('-') {
            i += 1;
            continue;
        }
        if builtins.iter().any(|b| b == word.as_ref()) {
            return args;
        }
        let Some(expansion) = aliases.get(word.as_ref()).and_then(|a| shlex::split(a)) else {
            return args;
        };
        let mut out = args[..i].to_vec();
        out.extend(expansion.into_iter().map(OsString::from));
        out.extend(args[i + 1..].iter().cloned());
        return out;
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn the_command_word_expands_past_global_options() {
        let aliases = BTreeMap::from([
            ("lg".to_string(), "log --oneline -n 5".to_string()),
            ("status".to_string(), "log".to_string()),
        ]);
        let builtins = vec!["status".to_string(), "log".to_string()];
        assert_eq!(
            expand(
                os(&["rill", "--repo", "lg", "lg", "src"]),
                &aliases,
                &builtins
            ),
            os(&["rill", "--repo", "lg", "log", "--oneline", "-n", "5", "src"])
        );
        assert_eq!(
            expand(os(&["rill", "status"]), &aliases, &builtins),
            os(&["rill", "status"]),
            "built-ins win"
        );
    }
}

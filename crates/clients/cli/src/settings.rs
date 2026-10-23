//! The CLI's own settings, from the same file as the client config
//! (`~/.config/rill/config.toml`, or `$RILL_CONFIG`): colours, the pager,
//! diff options and aliases.
//!
//! ```toml
//! [color]
//! ui = "auto"            # auto | always | never
//! added = "green bold"   # any theme role
//!
//! [core]
//! pager = "less -FRX"
//!
//! [diff]
//! syntax = true          # syntax highlighting in diffs
//!
//! [alias]
//! st = "status"
//! lg = "log --oneline"
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

/// Names the settings file instead of the default.
pub const CONFIG_ENV: &str = "RILL_CONFIG";

/// The settings, with defaults for anything unset.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Settings {
    /// `[color]`.
    pub color: ColorSettings,
    /// `[core]`.
    pub core: CoreSettings,
    /// `[diff]`.
    pub diff: DiffSettings,
    /// `[alias]`: name → the command words it stands for.
    pub alias: BTreeMap<String, String>,
}

/// `[color]`: when to colour, and per-role overrides.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ColorSettings {
    /// `auto`, `always` or `never`.
    pub ui: Option<String>,
    /// Theme role → colour spec.
    #[serde(flatten)]
    pub roles: BTreeMap<String, String>,
}

/// `[core]`.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct CoreSettings {
    /// The pager command.
    pub pager: Option<String>,
}

/// `[diff]`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DiffSettings {
    /// Syntax highlighting in diffs.
    pub syntax: bool,
}

impl Default for DiffSettings {
    fn default() -> Self {
        DiffSettings { syntax: true }
    }
}

impl Settings {
    /// The settings file's location.
    pub fn path() -> Option<PathBuf> {
        match std::env::var_os(CONFIG_ENV).filter(|p| !p.is_empty()) {
            Some(p) => Some(PathBuf::from(p)),
            None => rill_sdk::ClientConfig::default_path(),
        }
    }

    /// Read the settings file; a missing file gives the defaults and a
    /// malformed one is reported and ignored.
    pub fn load() -> Settings {
        let Some(path) = Self::path() else {
            return Settings::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).unwrap_or_else(|e| {
                tracing::warn!(file = %path.display(), error = %e, "settings ignored");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    /// Parse settings from TOML; sections the CLI does not own (`[server]`)
    /// are ignored.
    pub fn parse(text: &str) -> Result<Settings, toml::de::Error> {
        toml::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_parse_and_unknown_ones_are_ignored() {
        let s = Settings::parse(
            r#"
            [server]
            endpoint = "http://x"
            [color]
            ui = "always"
            added = "green bold"
            [core]
            pager = "most"
            [diff]
            syntax = false
            [alias]
            st = "status"
            "#,
        )
        .unwrap();
        assert_eq!(s.color.ui.as_deref(), Some("always"));
        assert_eq!(s.color.roles["added"], "green bold");
        assert_eq!(s.core.pager.as_deref(), Some("most"));
        assert!(!s.diff.syntax);
        assert_eq!(s.alias["st"], "status");
        assert!(Settings::parse("").unwrap().diff.syntax);
    }
}

//! Configuration of the risk classifier.

use std::collections::BTreeSet;

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use rill_types::{RepoPath, RiskClass};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A glob that marks matching paths as at least a given risk class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensitiveGlob {
    /// Glob over repository paths, e.g. `src/billing/**`.
    pub glob: String,
    /// Minimum class for a change touching a matching path.
    pub class: RiskClass,
}

/// Declarative risk configuration, typically loaded from `.rill/risk.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Globs that raise a change's floor.
    pub sensitive: Vec<SensitiveGlob>,
    /// Extensions (without the dot) whose files are documentation.
    pub trivial_extensions: BTreeSet<String>,
    /// Globs that identify test files or directories.
    pub test_globs: Vec<String>,
    /// More files than this bumps the class one step.
    pub max_files: u64,
    /// More lines than this bumps the class one step.
    pub max_lines: u64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        RiskConfig {
            sensitive: Vec::new(),
            trivial_extensions: ["md", "txt", "rst", "adoc"]
                .into_iter()
                .map(String::from)
                .collect(),
            test_globs: vec![
                "**/tests/**".into(),
                "**/test/**".into(),
                "**/*_test.*".into(),
                "**/*.test.*".into(),
                "**/*_spec.*".into(),
            ],
            max_files: 50,
            max_lines: 1000,
        }
    }
}

impl RiskConfig {
    /// Compile the globs. Fails on the first invalid pattern.
    pub fn compile(&self) -> Result<CompiledRiskConfig> {
        let mut sensitive = GlobSetBuilder::new();
        let mut classes = Vec::with_capacity(self.sensitive.len());
        for entry in &self.sensitive {
            sensitive.add(compile_glob(&entry.glob)?);
            classes.push((entry.glob.clone(), entry.class));
        }
        let mut tests = GlobSetBuilder::new();
        for glob in &self.test_globs {
            tests.add(compile_glob(glob)?);
        }
        Ok(CompiledRiskConfig {
            sensitive: sensitive.build().map_err(|e| Error::InvalidGlob {
                glob: "<sensitive>".into(),
                reason: e.to_string(),
            })?,
            sensitive_classes: classes,
            tests: tests.build().map_err(|e| Error::InvalidGlob {
                glob: "<tests>".into(),
                reason: e.to_string(),
            })?,
            trivial_extensions: self.trivial_extensions.clone(),
            max_files: self.max_files,
            max_lines: self.max_lines,
        })
    }
}

fn compile_glob(glob: &str) -> Result<Glob> {
    // `literal_separator` makes `*` stop at `/`, so `src/*.rs` does not
    // match `src/a/b.rs`; `**` still spans directories.
    GlobBuilder::new(glob)
        .literal_separator(true)
        .build()
        .map_err(|e| Error::InvalidGlob {
            glob: glob.to_string(),
            reason: e.to_string(),
        })
}

/// A [`RiskConfig`] with its globs compiled; build once, classify many.
#[derive(Debug, Clone)]
pub struct CompiledRiskConfig {
    sensitive: GlobSet,
    sensitive_classes: Vec<(String, RiskClass)>,
    tests: GlobSet,
    trivial_extensions: BTreeSet<String>,
    max_files: u64,
    max_lines: u64,
}

impl CompiledRiskConfig {
    /// Every sensitive glob that matches `path`, with its class.
    pub fn sensitive_matches(&self, path: &RepoPath) -> Vec<(&str, RiskClass)> {
        self.sensitive
            .matches(path.as_str())
            .into_iter()
            .map(|i| {
                let (glob, class) = &self.sensitive_classes[i];
                (glob.as_str(), *class)
            })
            .collect()
    }

    /// Whether `path` is a test file.
    pub fn is_test(&self, path: &RepoPath) -> bool {
        self.tests.is_match(path.as_str())
    }

    /// Whether `path` has a documentation extension.
    pub fn is_docs(&self, path: &RepoPath) -> bool {
        path.extension()
            .map(|ext| self.trivial_extensions.contains(&ext.to_ascii_lowercase()))
            .unwrap_or(false)
    }

    /// File-count threshold.
    pub fn max_files(&self) -> u64 {
        self.max_files
    }

    /// Line-count threshold.
    pub fn max_lines(&self) -> u64 {
        self.max_lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::RepoPathBuf;

    fn p(s: &str) -> RepoPathBuf {
        RepoPathBuf::new(s).unwrap()
    }

    #[test]
    fn default_config_compiles_and_classifies_paths() {
        let c = RiskConfig::default().compile().unwrap();
        assert!(c.is_test(&p("crates/x/tests/foo.rs")));
        assert!(c.is_test(&p("src/foo_test.go")));
        assert!(!c.is_test(&p("src/foo.go")));
        assert!(c.is_docs(&p("README.md")));
        assert!(c.is_docs(&p("docs/A.TXT")));
        assert!(!c.is_docs(&p("src/lib.rs")));
    }

    #[test]
    fn literal_separator_is_honored() {
        let cfg = RiskConfig {
            sensitive: vec![SensitiveGlob {
                glob: "src/*.rs".into(),
                class: RiskClass::Sensitive,
            }],
            ..Default::default()
        };
        let c = cfg.compile().unwrap();
        assert_eq!(c.sensitive_matches(&p("src/a.rs")).len(), 1);
        assert!(c.sensitive_matches(&p("src/a/b.rs")).is_empty());
    }

    #[test]
    fn invalid_glob_is_reported() {
        let cfg = RiskConfig {
            test_globs: vec!["[".into()],
            ..Default::default()
        };
        assert!(matches!(cfg.compile(), Err(Error::InvalidGlob { .. })));
    }
}

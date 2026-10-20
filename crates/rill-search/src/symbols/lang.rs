//! Language detection and per-language definition patterns.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use super::extract::SymbolKind;

/// Languages with a symbol extractor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
}

impl Language {
    /// Language of a path, by extension.
    pub fn from_path(path: &str) -> Option<Language> {
        let ext = path.rsplit('.').next()?;
        if ext.len() == path.len() {
            return None;
        }
        match ext {
            "rs" => Some(Language::Rust),
            "py" | "pyi" => Some(Language::Python),
            "ts" | "tsx" | "mts" | "cts" => Some(Language::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(Language::JavaScript),
            "go" => Some(Language::Go),
            _ => None,
        }
    }

    /// Definition patterns for this language. Each regex is anchored at
    /// the line start and has two capture groups: the keyword (which maps
    /// to a kind) and the name. Patterns without a keyword group use
    /// `kind` directly.
    pub(super) fn patterns(self) -> &'static [Pattern] {
        match self {
            Language::Rust => rust_patterns(),
            Language::Python => python_patterns(),
            Language::TypeScript | Language::JavaScript => js_patterns(),
            Language::Go => go_patterns(),
        }
    }
}

/// One line-anchored definition pattern.
pub(super) struct Pattern {
    pub regex: Regex,
    /// Kind when the regex has no keyword group.
    pub kind: Option<SymbolKind>,
}

impl Pattern {
    fn keyed(re: &str) -> Pattern {
        Pattern {
            regex: Regex::new(re).expect("static pattern compiles"),
            kind: None,
        }
    }

    fn fixed(re: &str, kind: SymbolKind) -> Pattern {
        Pattern {
            regex: Regex::new(re).expect("static pattern compiles"),
            kind: Some(kind),
        }
    }
}

/// Kind named by a definition keyword, across languages.
pub(super) fn kind_of_keyword(keyword: &str) -> SymbolKind {
    match keyword {
        "fn" | "def" | "func" | "function" | "function*" => SymbolKind::Function,
        "struct" => SymbolKind::Struct,
        "union" => SymbolKind::Struct,
        "enum" => SymbolKind::Enum,
        "trait" => SymbolKind::Trait,
        "mod" | "namespace" | "module" => SymbolKind::Module,
        "const" => SymbolKind::Const,
        "static" => SymbolKind::Static,
        "type" => SymbolKind::Type,
        "class" => SymbolKind::Class,
        "interface" => SymbolKind::Interface,
        _ => SymbolKind::Other,
    }
}

const IDENT: &str = r"[A-Za-z_][A-Za-z0-9_]*";

fn rust_patterns() -> &'static [Pattern] {
    static P: OnceLock<Vec<Pattern>> = OnceLock::new();
    P.get_or_init(|| {
        vec![
            Pattern::keyed(&format!(
                r#"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe|default|extern(?:\s+"[^"]*")?)\s+)*(fn|struct|enum|trait|mod|const|static|type|union)\s+({IDENT})"#
            )),
            // `impl<T> Trait for Type` names the implementing type.
            Pattern::fixed(
                &format!(
                    r"^\s*(?:unsafe\s+)?impl(?:<[^>]*>)?\s+(?:!?[A-Za-z_][A-Za-z0-9_:]*(?:<[^>]*>)?\s+for\s+)?({IDENT})"
                ),
                SymbolKind::Impl,
            ),
        ]
    })
}

fn python_patterns() -> &'static [Pattern] {
    static P: OnceLock<Vec<Pattern>> = OnceLock::new();
    P.get_or_init(|| {
        vec![Pattern::keyed(&format!(
            r"^\s*(?:async\s+)?(def|class)\s+({IDENT})"
        ))]
    })
}

fn js_patterns() -> &'static [Pattern] {
    static P: OnceLock<Vec<Pattern>> = OnceLock::new();
    const JS_IDENT: &str = r"[A-Za-z_$][A-Za-z0-9_$]*";
    P.get_or_init(|| {
        vec![
            Pattern::keyed(&format!(
                r"^\s*(?:export\s+)?(?:default\s+)?(?:declare\s+)?(?:abstract\s+)?(?:async\s+)?(function\*?|class|interface|type|enum|namespace|module)\s+({JS_IDENT})"
            )),
            // `const f = (…) =>`, `const f = async function`, `const f = x =>`.
            Pattern::fixed(
                &format!(
                    r"^\s*(?:export\s+)?(?:const|let|var)\s+({JS_IDENT})\s*(?::[^=]+)?=\s*(?:async\s+)?(?:function\b|\([^)]*\)\s*(?::[^=]+)?=>|{JS_IDENT}\s*=>)"
                ),
                SymbolKind::Function,
            ),
        ]
    })
}

fn go_patterns() -> &'static [Pattern] {
    static P: OnceLock<Vec<Pattern>> = OnceLock::new();
    P.get_or_init(|| {
        vec![Pattern::keyed(&format!(
            r"^\s*(func|type)\s+(?:\([^)]*\)\s*)?({IDENT})"
        ))]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_by_extension() {
        assert_eq!(Language::from_path("src/lib.rs"), Some(Language::Rust));
        assert_eq!(Language::from_path("a/b.tsx"), Some(Language::TypeScript));
        assert_eq!(Language::from_path("Makefile"), None);
        assert_eq!(Language::from_path("x.unknown"), None);
    }

    #[test]
    fn patterns_compile() {
        for lang in [
            Language::Rust,
            Language::Python,
            Language::TypeScript,
            Language::Go,
        ] {
            assert!(!lang.patterns().is_empty());
        }
    }
}

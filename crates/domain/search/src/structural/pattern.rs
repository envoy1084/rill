//! A compiled structural pattern: what it requires of a file's text, and
//! where it matches in a file.

use std::ops::Range;

use ast_grep_core::matcher::Pattern;
use ast_grep_core::tree_sitter::LanguageExt;

use super::grammar::{structural_languages, Grammar};
use crate::error::{Error, Result};
use crate::trigram::TrigramQuery;

/// An ast-grep pattern for one language.
#[derive(Clone)]
pub struct StructuralPattern {
    language: &'static str,
    source: String,
    /// One compiled form per grammar the language parses with (TypeScript
    /// has two).
    compiled: Vec<(Grammar, Pattern, bool)>,
}

impl std::fmt::Debug for StructuralPattern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StructuralPattern")
            .field("language", &self.language)
            .field("source", &self.source)
            .finish()
    }
}

impl StructuralPattern {
    /// Compile `source` for `language` (case-insensitive).
    pub fn new(source: &str, language: &str) -> Result<StructuralPattern> {
        let language = structural_languages()
            .iter()
            .copied()
            .find(|l| l.eq_ignore_ascii_case(language))
            .ok_or_else(|| {
                Error::InvalidPattern(format!(
                    "structural search does not support {language:?}; it supports {}",
                    structural_languages().join(", ")
                ))
            })?;
        let mut compiled = Vec::new();
        let paths: &[(&str, bool)] = match language {
            "TypeScript" => &[("a.ts", false), ("a.tsx", true)],
            _ => &[("a", false)],
        };
        for (path, tsx) in paths {
            let grammar =
                Grammar::for_file(language, path).expect("supported languages have grammars");
            let pattern = Pattern::try_new(source, grammar.clone())
                .map_err(|e| Error::InvalidPattern(format!("structural pattern: {e}")))?;
            compiled.push((grammar, pattern, *tsx));
        }
        Ok(StructuralPattern {
            language,
            source: source.to_string(),
            compiled,
        })
    }

    /// The language the pattern is written in.
    pub fn language(&self) -> &'static str {
        self.language
    }

    /// Trigrams a matching file must contain: those of every identifier
    /// and literal token of the pattern that is not a meta-variable.
    pub fn trigram_query(&self) -> TrigramQuery {
        let mut terms = Vec::new();
        let mut token = String::new();
        let mut in_var = false;
        for c in self.source.chars().chain(std::iter::once(' ')) {
            if c.is_alphanumeric() || c == '_' {
                token.push(c);
                continue;
            }
            if !in_var && token.len() >= 3 {
                terms.push(TrigramQuery::literal(token.as_bytes()));
            }
            token.clear();
            in_var = c == '$';
        }
        TrigramQuery::and(terms)
    }

    /// Byte ranges where the pattern matches the file at `path`; none when
    /// the file is not UTF-8 or the pattern's language.
    pub fn find(&self, path: &str, bytes: &[u8]) -> Vec<Range<usize>> {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Vec::new();
        };
        let tsx = path.ends_with(".tsx");
        let Some((grammar, pattern, _)) = self
            .compiled
            .iter()
            .find(|(_, _, for_tsx)| *for_tsx == tsx)
            .or(self.compiled.first())
        else {
            return Vec::new();
        };
        let root = grammar.ast_grep(text);
        root.root()
            .find_all(pattern.clone())
            .map(|m| m.range())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_variables_match_any_subtree() {
        let p = StructuralPattern::new("foo($A, $$$REST)", "rust").unwrap();
        let src = b"fn f() { foo(1, 2, 3); bar(1, 2); foo(x + y, z); }";
        let found: Vec<_> = p
            .find("a.rs", src)
            .into_iter()
            .map(|r| String::from_utf8_lossy(&src[r]).into_owned())
            .collect();
        assert_eq!(found, vec!["foo(1, 2, 3)", "foo(x + y, z)"]);
    }

    #[test]
    fn python_patterns_use_the_expando() {
        let p = StructuralPattern::new("print($X)", "Python").unwrap();
        assert_eq!(p.find("a.py", b"print(1)\nprint(a, b)\n").len(), 1);
    }

    #[test]
    fn typescript_matches_tsx_files() {
        let p = StructuralPattern::new("useState($X)", "typescript").unwrap();
        let src = b"const [a, b] = useState(0); const el = <div>{a}</div>;";
        assert_eq!(p.find("c.tsx", src).len(), 1);
    }

    #[test]
    fn unsupported_languages_are_refused() {
        assert!(StructuralPattern::new("x", "Cobol").is_err());
    }
}

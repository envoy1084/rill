//! The tree-sitter grammars structural search parses with, adapted to
//! ast-grep's `Language`.

use std::borrow::Cow;

use ast_grep_core::matcher::{Pattern, PatternBuilder, PatternError};
use ast_grep_core::tree_sitter::{LanguageExt, StrDoc, TSLanguage};
use ast_grep_core::Language;

/// One grammar, as ast-grep sees it.
#[derive(Clone)]
pub struct Grammar {
    ts: TSLanguage,
    /// Stands in for `$` in patterns where `$` cannot start an identifier.
    expando: char,
}

/// The languages (as [`crate::language_of`] names them) structural search
/// supports.
pub fn structural_languages() -> &'static [&'static str] {
    &[
        "Rust",
        "Go",
        "Python",
        "JavaScript",
        "TypeScript",
        "Java",
        "C",
        "C++",
        "Ruby",
    ]
}

impl Grammar {
    /// The grammar for a file of `language` at `path` (TypeScript files
    /// with JSX parse as TSX).
    pub fn for_file(language: &str, path: &str) -> Option<Grammar> {
        let dollar = |ts: TSLanguage| Grammar { ts, expando: '$' };
        let expando = |ts: TSLanguage| Grammar { ts, expando: 'µ' };
        Some(match language {
            "Rust" => expando(tree_sitter_rust::LANGUAGE.into()),
            "Go" => expando(tree_sitter_go::LANGUAGE.into()),
            "Python" => expando(tree_sitter_python::LANGUAGE.into()),
            "JavaScript" => dollar(tree_sitter_javascript::LANGUAGE.into()),
            "TypeScript" if path.ends_with(".tsx") => {
                dollar(tree_sitter_typescript::LANGUAGE_TSX.into())
            }
            "TypeScript" => dollar(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            "Java" => dollar(tree_sitter_java::LANGUAGE.into()),
            "C" => expando(tree_sitter_c::LANGUAGE.into()),
            "C++" => expando(tree_sitter_cpp::LANGUAGE.into()),
            "Ruby" => expando(tree_sitter_ruby::LANGUAGE.into()),
            _ => return None,
        })
    }
}

impl Language for Grammar {
    fn pre_process_pattern<'q>(&self, query: &'q str) -> Cow<'q, str> {
        if self.expando == '$' {
            return Cow::Borrowed(query);
        }
        let mut out = String::with_capacity(query.len());
        let mut chars = query.chars().peekable();
        while let Some(c) = chars.next() {
            let starts_var = c == '$'
                && chars
                    .peek()
                    .is_some_and(|n| n.is_ascii_uppercase() || *n == '_' || *n == '$');
            out.push(if starts_var { self.expando } else { c });
        }
        Cow::Owned(out)
    }

    fn expando_char(&self) -> char {
        self.expando
    }

    fn kind_to_id(&self, kind: &str) -> u16 {
        self.ts.id_for_node_kind(kind, true)
    }

    fn field_to_id(&self, field: &str) -> Option<u16> {
        self.ts.field_id_for_name(field).map(|f| f.get())
    }

    fn build_pattern(&self, builder: &PatternBuilder) -> Result<Pattern, PatternError> {
        builder.build(|src| StrDoc::try_new(src, self.clone()))
    }
}

impl LanguageExt for Grammar {
    fn get_ts_language(&self) -> TSLanguage {
        self.ts.clone()
    }
}

//! Definitions in a file, from sem-core's tree-sitter entity extraction.

use std::sync::OnceLock;

use sem_core::parser::plugins::create_default_registry;
use sem_core::parser::registry::ParserRegistry;
use serde::{Deserialize, Serialize};

/// What a symbol defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Class,
    Struct,
    Enum,
    Trait,
    Impl,
    Interface,
    Module,
    Const,
    Static,
    Type,
    Other,
}

impl SymbolKind {
    const ALL: [SymbolKind; 12] = [
        SymbolKind::Function,
        SymbolKind::Class,
        SymbolKind::Struct,
        SymbolKind::Enum,
        SymbolKind::Trait,
        SymbolKind::Impl,
        SymbolKind::Interface,
        SymbolKind::Module,
        SymbolKind::Const,
        SymbolKind::Static,
        SymbolKind::Type,
        SymbolKind::Other,
    ];

    /// The kind's lowercase name (`function`, `class`, …).
    pub fn as_str(self) -> &'static str {
        match self {
            SymbolKind::Function => "function",
            SymbolKind::Class => "class",
            SymbolKind::Struct => "struct",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Impl => "impl",
            SymbolKind::Interface => "interface",
            SymbolKind::Module => "module",
            SymbolKind::Const => "const",
            SymbolKind::Static => "static",
            SymbolKind::Type => "type",
            SymbolKind::Other => "other",
        }
    }

    /// The kind [`SymbolKind::as_str`] names (case-insensitive).
    pub fn parse(name: &str) -> Option<SymbolKind> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(name))
    }
}

/// A definition found in a file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    /// 1-based line of the definition.
    pub line: u32,
}

fn registry() -> &'static ParserRegistry {
    static REGISTRY: OnceLock<ParserRegistry> = OnceLock::new();
    REGISTRY.get_or_init(create_default_registry)
}

/// Whether `path` is source code sem-core parses (data and prose formats
/// are left out of the symbol table).
pub fn is_source(path: &str) -> bool {
    registry()
        .get_plugin(path)
        .is_some_and(|p| p.id() == "code")
}

/// Definitions in `bytes`, parsed as the language `path` implies. Files
/// that are not source code, binary or not UTF-8 yield nothing.
pub fn extract_symbols(path: &str, bytes: &[u8]) -> Vec<Symbol> {
    if !is_source(path) || rill_core::is_binary(bytes) {
        return Vec::new();
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Vec::new();
    };
    registry()
        .extract_entities_brief(path, text)
        .into_iter()
        .filter(|e| !e.name.is_empty())
        .map(|e| Symbol {
            kind: kind_of(&e.entity_type),
            name: e.name,
            line: e.start_line as u32,
        })
        .collect()
}

fn kind_of(entity_type: &str) -> SymbolKind {
    let t = entity_type.to_ascii_lowercase();
    let has = |w: &str| t.contains(w);
    if has("function") || has("method") || has("constructor") {
        SymbolKind::Function
    } else if has("interface") || has("protocol") {
        SymbolKind::Interface
    } else if has("trait") {
        SymbolKind::Trait
    } else if has("impl") {
        SymbolKind::Impl
    } else if has("struct") {
        SymbolKind::Struct
    } else if has("enum") {
        SymbolKind::Enum
    } else if has("class") || has("object") {
        SymbolKind::Class
    } else if has("module") || has("namespace") || has("package") || t == "mod" {
        SymbolKind::Module
    } else if has("const") {
        SymbolKind::Const
    } else if has("static") {
        SymbolKind::Static
    } else if has("type") {
        SymbolKind::Type
    } else {
        SymbolKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(path: &str, src: &str) -> Vec<(String, SymbolKind, u32)> {
        extract_symbols(path, src.as_bytes())
            .into_iter()
            .map(|s| (s.name, s.kind, s.line))
            .collect()
    }

    #[test]
    fn definitions_come_from_source_files_only() {
        let rust = found("src/lib.rs", "pub struct Foo;\n\nfn run() {}\n");
        assert!(
            rust.contains(&("Foo".into(), SymbolKind::Struct, 1)),
            "{rust:?}"
        );
        assert!(
            rust.contains(&("run".into(), SymbolKind::Function, 3)),
            "{rust:?}"
        );
        let python = found(
            "app/views.py",
            "class View:\n    def get(self):\n        pass\n",
        );
        assert!(
            python
                .iter()
                .any(|(n, k, _)| n == "View" && *k == SymbolKind::Class),
            "{python:?}"
        );
        let ts = found("web/api.ts", "export function load() {}\n");
        assert!(
            ts.iter()
                .any(|(n, k, _)| n == "load" && *k == SymbolKind::Function),
            "{ts:?}"
        );
        assert!(
            found("config/app.json", "{\"a\": 1}").is_empty(),
            "data files are not symbols"
        );
        assert!(found("README", "fn main() {}").is_empty());
    }
}

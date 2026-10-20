//! Regex-based definition extraction.

use serde::{Deserialize, Serialize};

use super::lang::{kind_of_keyword, Language};

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

/// A definition found in a file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    /// 1-based line of the definition.
    pub line: u32,
}

/// Definitions in `bytes`, using the language implied by `path`'s
/// extension. Unknown languages and binary content yield nothing.
pub fn extract_symbols(path: &str, bytes: &[u8]) -> Vec<Symbol> {
    let Some(lang) = Language::from_path(path) else {
        return Vec::new();
    };
    extract_for(lang, bytes)
}

/// Definitions in `bytes` for a known language.
pub fn extract_for(lang: Language, bytes: &[u8]) -> Vec<Symbol> {
    if rill_core::is_binary(bytes) {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(bytes);
    let patterns = lang.patterns();
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        // Cheap pre-check: every pattern needs a letter on the line.
        if !line.bytes().any(|b| b.is_ascii_alphabetic()) {
            continue;
        }
        for pattern in patterns {
            let Some(caps) = pattern.regex.captures(line) else {
                continue;
            };
            let (kind, name) = match pattern.kind {
                Some(kind) => (kind, caps.get(1)),
                None => (
                    kind_of_keyword(caps.get(1).map_or("", |m| m.as_str())),
                    caps.get(2),
                ),
            };
            if let Some(name) = name {
                out.push(Symbol {
                    name: name.as_str().to_string(),
                    kind,
                    line: (i + 1) as u32,
                });
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(path: &str, src: &str) -> Vec<(String, SymbolKind, u32)> {
        extract_symbols(path, src.as_bytes())
            .into_iter()
            .map(|s| (s.name, s.kind, s.line))
            .collect()
    }

    #[test]
    fn rust_definitions() {
        let src = "pub struct Foo;\n\nimpl<T: Clone> Display for Foo {\n    pub(crate) async fn run(&self) {}\n}\nconst MAX: u32 = 1;\npub unsafe trait Raw {}\nmod inner;\nenum E { A }\ntype Alias = u8;\nstatic S: u8 = 0;\n";
        assert_eq!(
            names("a.rs", src),
            vec![
                ("Foo".into(), SymbolKind::Struct, 1),
                ("Foo".into(), SymbolKind::Impl, 3),
                ("run".into(), SymbolKind::Function, 4),
                ("MAX".into(), SymbolKind::Const, 6),
                ("Raw".into(), SymbolKind::Trait, 7),
                ("inner".into(), SymbolKind::Module, 8),
                ("E".into(), SymbolKind::Enum, 9),
                ("Alias".into(), SymbolKind::Type, 10),
                ("S".into(), SymbolKind::Static, 11),
            ]
        );
    }

    #[test]
    fn python_definitions() {
        let src = "class A:\n    def m(self):\n        pass\nasync def go(): ...\n";
        assert_eq!(
            names("a.py", src),
            vec![
                ("A".into(), SymbolKind::Class, 1),
                ("m".into(), SymbolKind::Function, 2),
                ("go".into(), SymbolKind::Function, 4),
            ]
        );
    }

    #[test]
    fn typescript_definitions() {
        let src = "export default class Widget {}\nexport interface Props {}\ntype Id = string;\nexport const load = async (x: number): Promise<void> => {};\nconst f = function () {};\nlet g = x => x;\nconst notFn = 3;\nfunction* gen() {}\n";
        assert_eq!(
            names("w.ts", src),
            vec![
                ("Widget".into(), SymbolKind::Class, 1),
                ("Props".into(), SymbolKind::Interface, 2),
                ("Id".into(), SymbolKind::Type, 3),
                ("load".into(), SymbolKind::Function, 4),
                ("f".into(), SymbolKind::Function, 5),
                ("g".into(), SymbolKind::Function, 6),
                ("gen".into(), SymbolKind::Function, 8),
            ]
        );
    }

    #[test]
    fn go_definitions() {
        let src =
            "package x\n\nfunc (s *Server) Start() {}\nfunc main() {}\ntype Server struct{}\n";
        assert_eq!(
            names("s.go", src),
            vec![
                ("Start".into(), SymbolKind::Function, 3),
                ("main".into(), SymbolKind::Function, 4),
                ("Server".into(), SymbolKind::Type, 5),
            ]
        );
    }

    #[test]
    fn unknown_and_binary_yield_nothing() {
        assert!(names("README", "fn main() {}").is_empty());
        assert!(extract_symbols("a.rs", b"fn main\0() {}").is_empty());
    }
}

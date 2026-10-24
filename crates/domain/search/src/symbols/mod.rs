//! Definitions in source files, extracted with sem-core's tree-sitter
//! entity extraction and stored with each blob in its segment.

mod extract;
mod lookup;

pub use extract::{extract_symbols, is_source, Symbol, SymbolKind};
pub use lookup::{file_symbols, find_symbols, SymbolHit, SymbolName, SymbolQuery};

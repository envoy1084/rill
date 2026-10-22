//! Symbol extraction and lookup. Definitions come from sem-core's
//! tree-sitter entity extraction (about thirty languages); the index and
//! its stored per-root tables are Rill's.

mod extract;
mod index;

pub use extract::{extract_symbols, is_source, Symbol, SymbolKind};
pub use index::{RootSymbols, SymbolIndex, SymbolLocation, KIND_SYMBOLS};

//! Lightweight symbol extraction and lookup.
//!
//! Definitions are found with per-language line regexes, which keeps the
//! dependency footprint tiny and is accurate enough for "jump to the
//! definition of `foo`" over a codebase. The planned upgrade path is
//! tree-sitter grammars (or ingesting SCIP indexes produced by CI) behind
//! the same [`extract_symbols`] / [`SymbolIndex`] surface.

mod extract;
mod index;
mod lang;

pub use extract::{extract_symbols, Symbol, SymbolKind};
pub use index::{RootSymbols, SymbolIndex, SymbolLocation, KIND_SYMBOLS};
pub use lang::Language;

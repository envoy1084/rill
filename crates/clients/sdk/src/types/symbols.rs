//! Code intelligence types: symbol locations and reference lists.

use rill_proto::convert::path_from_proto;
use rill_proto::rill::v1 as pb;
use rill_types::RepoPathBuf;

/// A symbol definition, or a use of one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocation {
    /// The file.
    pub path: RepoPathBuf,
    /// One-based line.
    pub line: u32,
    /// The symbol's name.
    pub name: String,
    /// `function`, `class`, … (empty for a use).
    pub kind: String,
    /// Zero-based byte columns on the line, when known.
    pub start_column: u32,
    /// End of the name on the line.
    pub end_column: u32,
    /// From an uploaded SCIP index rather than extraction or text.
    pub precise: bool,
    /// The location defines the symbol.
    pub definition: bool,
    /// The SCIP symbol, when precise.
    pub symbol: String,
}

impl TryFrom<pb::SymbolLocation> for SymbolLocation {
    type Error = rill_proto::ConvertError;

    fn try_from(s: pb::SymbolLocation) -> std::result::Result<Self, Self::Error> {
        Ok(SymbolLocation {
            path: path_from_proto("symbol.path", &s.path)?,
            line: s.line,
            name: s.name,
            kind: s.kind,
            start_column: s.start_column,
            end_column: s.end_column,
            precise: s.precise,
            definition: s.definition,
            symbol: s.symbol,
        })
    }
}

/// What to find references to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceTarget {
    /// Every symbol with this name.
    Name(String),
    /// The symbol at a one-based line and zero-based column of a file.
    Position {
        /// The file.
        path: RepoPathBuf,
        /// One-based line.
        line: u32,
        /// Zero-based byte column.
        column: u32,
    },
}

/// The uses of a symbol.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct References {
    /// By path, then position.
    pub references: Vec<SymbolLocation>,
    /// Every reference came from a SCIP index (else they are whole-word
    /// text matches).
    pub precise: bool,
    /// More exist than were returned.
    pub truncated: bool,
}

/// What a SCIP upload covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScipUpload {
    /// Files of the revision the index covers.
    pub files: u32,
    /// Occurrences kept.
    pub occurrences: u64,
}

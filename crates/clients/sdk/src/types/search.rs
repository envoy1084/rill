//! Code search types.

use rill_proto::convert::path_from_proto;
use rill_proto::rill::v1 as pb;
use rill_types::RepoPathBuf;

use crate::error::Result;

/// A regex search over the files of a root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepQuery {
    /// Regular expression in Rust `regex` syntax.
    pub pattern: String,
    /// Match without regard to ASCII case.
    pub case_insensitive: bool,
    /// Glob filters on paths; empty matches everything.
    pub path_globs: Vec<String>,
    /// Maximum hits; `0` lets the server pick its default.
    pub max_results: u32,
    /// Lines of context around each hit.
    pub context_lines: u32,
    /// Match `pattern` literally, not as a regex.
    pub literal: bool,
    /// Only files in these languages; empty is every language.
    pub languages: Vec<String>,
    /// Wait until trunk is indexed through this position.
    pub min_seq: Option<u64>,
    /// `pattern` is an ast-grep structural pattern; set exactly one
    /// language.
    pub structural: bool,
}

impl GrepQuery {
    /// A query for `pattern` with no filters and server defaults.
    pub fn new(pattern: impl Into<String>) -> Self {
        GrepQuery {
            pattern: pattern.into(),
            case_insensitive: false,
            path_globs: Vec::new(),
            max_results: 0,
            context_lines: 0,
            literal: false,
            languages: Vec::new(),
            min_seq: None,
            structural: false,
        }
    }

    /// Match `pattern` as an ast-grep structural pattern
    /// (`foo($A, $$$REST)`) in the one language set with
    /// [`GrepQuery::language`].
    pub fn structural(mut self, yes: bool) -> Self {
        self.structural = yes;
        self
    }

    /// Match literally.
    pub fn literal(mut self, yes: bool) -> Self {
        self.literal = yes;
        self
    }

    /// Restrict results to a language (`rust`, `python`, …).
    pub fn language(mut self, lang: impl Into<String>) -> Self {
        self.languages.push(lang.into());
        self
    }

    /// Wait until trunk is indexed through `seq` first.
    pub fn min_seq(mut self, seq: u64) -> Self {
        self.min_seq = Some(seq);
        self
    }

    /// Match case-insensitively.
    pub fn case_insensitive(mut self, yes: bool) -> Self {
        self.case_insensitive = yes;
        self
    }

    /// Restrict results to paths matching `glob`.
    pub fn glob(mut self, glob: impl Into<String>) -> Self {
        self.path_globs.push(glob.into());
        self
    }

    /// Cap the number of hits.
    pub fn max_results(mut self, n: u32) -> Self {
        self.max_results = n;
        self
    }

    /// Lines of context to return around each hit.
    pub fn context_lines(mut self, n: u32) -> Self {
        self.context_lines = n;
        self
    }
}

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepHit {
    /// File containing the match.
    pub path: RepoPathBuf,
    /// One-based line number.
    pub line_no: u32,
    /// The matching line without its terminator.
    pub line: String,
    /// Lines preceding the match.
    pub before: Vec<String>,
    /// Lines following the match.
    pub after: Vec<String>,
    /// Byte offsets of the first match on the line within the whole file.
    pub byte_start: u64,
    /// End of that match.
    pub byte_end: u64,
}

impl TryFrom<pb::GrepHit> for GrepHit {
    type Error = rill_proto::ConvertError;

    fn try_from(h: pb::GrepHit) -> std::result::Result<Self, Self::Error> {
        Ok(GrepHit {
            path: path_from_proto("grep_hit.path", &h.path)?,
            line_no: h.line_no,
            line: h.line,
            before: h.before,
            after: h.after,
            byte_start: h.byte_start,
            byte_end: h.byte_end,
        })
    }
}

/// Result of a search.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrepResult {
    /// Best files first, then line order within a file.
    pub hits: Vec<GrepHit>,
    /// More hits exist than were returned.
    pub truncated: bool,
    /// Trunk is indexed through this position.
    pub indexed_through: u64,
    /// Files with a match, among those looked at.
    pub files_matched: u32,
    /// The server's time budget ran out first.
    pub partial: bool,
    /// Matched files by top-level directory.
    pub directories: std::collections::BTreeMap<String, u32>,
    /// Matched files by language.
    pub languages: std::collections::BTreeMap<String, u32>,
}

impl GrepResult {
    pub(crate) fn from_proto(resp: pb::GrepResponse) -> Result<Self> {
        let hits = resp
            .hits
            .into_iter()
            .map(GrepHit::try_from)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(GrepResult {
            hits,
            truncated: resp.truncated,
            indexed_through: resp.indexed_through,
            files_matched: resp.files_matched,
            partial: resp.partial,
            directories: resp.directories.into_iter().collect(),
            languages: resp.languages.into_iter().collect(),
        })
    }
}

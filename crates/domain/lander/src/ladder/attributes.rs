//! The `merge` attribute of a repository's root `.gitattributes`, read
//! with gitoxide's attribute matcher so patterns mean what they mean to
//! git. Nested `.gitattributes` files are not read.

use gix_attributes::search::{MetadataCollection, Outcome};
use gix_attributes::{Search, StateRef};

/// Where the attributes live.
pub const GITATTRIBUTES_PATH: &str = ".gitattributes";

/// What `.gitattributes` says about merging a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeAttribute {
    /// `merge=union`.
    Union,
    /// `-merge` or `merge=binary`: never merged automatically.
    Manual,
    /// Anything else, or nothing.
    Unspecified,
}

/// A parsed `.gitattributes`.
#[derive(Clone)]
pub struct GitAttributes {
    search: Search,
    collection: MetadataCollection,
}

impl std::fmt::Debug for GitAttributes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GitAttributes")
    }
}

impl GitAttributes {
    /// Parse the file's bytes; `None` when they do not parse.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let mut search = Search::default();
        let mut collection = MetadataCollection::default();
        if let Err(e) = search.add_patterns_buffer(
            bytes,
            GITATTRIBUTES_PATH.into(),
            None,
            &mut collection,
            true,
        ) {
            tracing::warn!(error = %e, "unreadable .gitattributes; ignoring it");
            return None;
        }
        Some(GitAttributes { search, collection })
    }

    /// The merge attribute of `path`.
    pub fn merge(&self, path: &str) -> MergeAttribute {
        let mut out = Outcome::default();
        out.initialize_with_selection(&self.collection, ["merge"]);
        self.search.pattern_matching_relative_path(
            path.into(),
            gix_attributes::glob::pattern::Case::Sensitive,
            Some(false),
            &mut out,
        );
        let Some(found) = out.iter_selected().next() else {
            return MergeAttribute::Unspecified;
        };
        match found.assignment.state {
            StateRef::Unset => MergeAttribute::Manual,
            StateRef::Value(v) if v.as_bstr() == "union" => MergeAttribute::Union,
            StateRef::Value(v) if v.as_bstr() == "binary" => MergeAttribute::Manual,
            _ => MergeAttribute::Unspecified,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_merge_attribute_as_git_does() {
        let attrs = GitAttributes::parse(
            b"CHANGELOG.md merge=union\n*.png -merge\n/vendor/** merge=binary\n*.rs text\n",
        )
        .unwrap();
        assert_eq!(attrs.merge("CHANGELOG.md"), MergeAttribute::Union);
        assert_eq!(attrs.merge("docs/CHANGELOG.md"), MergeAttribute::Union);
        assert_eq!(attrs.merge("img/a.png"), MergeAttribute::Manual);
        assert_eq!(attrs.merge("vendor/x/y.c"), MergeAttribute::Manual);
        assert_eq!(attrs.merge("src/a.rs"), MergeAttribute::Unspecified);
    }
}

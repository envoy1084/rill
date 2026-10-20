//! Parsing of the positional values commands share: roots, sequence
//! numbers and "a change id or a version hash".

use rill_sdk::{RepoClient, Root};
use rill_types::{ChangeId, ChangeInfo, ChangeVersionHash, TrunkSeq};

use crate::error::CliError;
use crate::Result;

/// Parse a root argument (`trunk`, `trunk@N`, hash, ref), trunk when absent.
pub fn root_arg(text: Option<&str>) -> Result<Root> {
    match text {
        None => Ok(Root::trunk()),
        Some(t) => t
            .parse()
            .map_err(|e: rill_sdk::Error| CliError::usage(format!("invalid root {t:?}: {e}"))),
    }
}

/// Parse a trunk sequence (`7` or `trunk@7`).
pub fn seq_arg(text: &str) -> Result<TrunkSeq> {
    text.parse()
        .map_err(|_| CliError::usage(format!("invalid trunk sequence {text:?}")))
}

/// A change named either by stable id or by version hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeRef {
    /// `ch_<hex>`.
    Id(ChangeId),
    /// A 64-hex version hash.
    Version(ChangeVersionHash),
}

impl ChangeRef {
    /// Resolve to the change's current (or named) version info.
    pub async fn info(self, repo: &RepoClient) -> Result<ChangeInfo> {
        Ok(match self {
            ChangeRef::Id(id) => repo.current_version(id).await?,
            ChangeRef::Version(h) => repo.change_info(h).await?,
        })
    }

    /// Resolve to the change id.
    pub async fn id(self, repo: &RepoClient) -> Result<ChangeId> {
        Ok(match self {
            ChangeRef::Id(id) => id,
            ChangeRef::Version(h) => repo.change_info(h).await?.change_id,
        })
    }
}

/// Parse `ch_<hex>` or a version hash.
pub fn change_ref(text: &str) -> Result<ChangeRef> {
    if text.starts_with("ch_") {
        return text
            .parse()
            .map(ChangeRef::Id)
            .map_err(|e| CliError::usage(format!("invalid change id {text:?}: {e}")));
    }
    if text.len() == 64 {
        if let Ok(h) = ChangeVersionHash::from_hex(text) {
            return Ok(ChangeRef::Version(h));
        }
    }
    text.parse().map(ChangeRef::Id).map_err(|_| {
        CliError::usage(format!(
            "{text:?} is neither a change id nor a version hash"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ids_and_hashes() {
        let id = ChangeId::new();
        assert_eq!(change_ref(&id.to_string()).unwrap(), ChangeRef::Id(id));
        assert_eq!(change_ref(&id.to_hex()).unwrap(), ChangeRef::Id(id));
        let h = ChangeVersionHash::digest(rill_types::ObjectKind::Blob, b"x");
        assert_eq!(change_ref(&h.to_hex()).unwrap(), ChangeRef::Version(h));
        assert!(change_ref("nope").is_err());
        assert_eq!(seq_arg("trunk@3").unwrap(), TrunkSeq(3));
        assert_eq!(seq_arg("3").unwrap(), TrunkSeq(3));
        assert!(matches!(
            root_arg(Some("trunk@2")).unwrap(),
            Root::Seq(TrunkSeq(2))
        ));
        assert_eq!(root_arg(None).unwrap(), Root::trunk());
    }
}

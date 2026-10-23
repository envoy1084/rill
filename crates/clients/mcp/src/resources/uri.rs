//! Resource URIs:
//!
//! * `rill://{repo}/files/{root}/{path}`: a file's content at a root;
//! * `rill://{repo}/changes/{change}/diff`: a change's diff;
//! * `rill://{repo}/changes/{change}/review`: its review status.
//!
//! The repository and root are single segments, so a `/` in them is
//! written `%2F`.

use rill_types::{ChangeId, RepoPathBuf};

use crate::error::{ToolError, ToolResult};

const SCHEME: &str = "rill://";

/// A parsed resource URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceUri {
    /// A file at a root.
    File {
        /// Repository.
        repo: String,
        /// Root spec, as `rill_read` takes it.
        root: String,
        /// The file.
        path: RepoPathBuf,
    },
    /// A change's diff against its parent.
    Diff {
        /// Repository.
        repo: String,
        /// The change.
        change: ChangeId,
    },
    /// A change's review status.
    Review {
        /// Repository.
        repo: String,
        /// The change.
        change: ChangeId,
    },
}

impl ResourceUri {
    /// Parse `uri`.
    pub fn parse(uri: &str) -> ToolResult<Self> {
        let bad = || ToolError::invalid(format!("not a rill resource URI: {uri}"));
        let rest = uri.strip_prefix(SCHEME).ok_or_else(bad)?;
        let mut parts = rest.splitn(3, '/');
        let repo = decode(parts.next().filter(|r| !r.is_empty()).ok_or_else(bad)?)?;
        match (parts.next(), parts.next()) {
            (Some("files"), Some(rest)) => {
                let (root, path) = rest.split_once('/').ok_or_else(bad)?;
                Ok(ResourceUri::File {
                    repo,
                    root: decode(root)?,
                    path: RepoPathBuf::new(path)?,
                })
            }
            (Some("changes"), Some(rest)) => {
                let (change, what) = rest.split_once('/').ok_or_else(bad)?;
                let change = change
                    .parse()
                    .map_err(|e| ToolError::invalid(format!("{uri}: {e}")))?;
                match what {
                    "diff" => Ok(ResourceUri::Diff { repo, change }),
                    "review" => Ok(ResourceUri::Review { repo, change }),
                    _ => Err(bad()),
                }
            }
            _ => Err(bad()),
        }
    }

    /// The URI text.
    pub fn to_uri(&self) -> String {
        match self {
            ResourceUri::File { repo, root, path } => {
                format!("{SCHEME}{}/files/{}/{path}", encode(repo), encode(root))
            }
            ResourceUri::Diff { repo, change } => {
                format!("{SCHEME}{}/changes/{change}/diff", encode(repo))
            }
            ResourceUri::Review { repo, change } => {
                format!("{SCHEME}{}/changes/{change}/review", encode(repo))
            }
        }
    }

    /// The change whose review events update this resource, if any.
    pub fn watched_change(&self) -> Option<(&str, ChangeId)> {
        match self {
            ResourceUri::Diff { repo, change } | ResourceUri::Review { repo, change } => {
                Some((repo, *change))
            }
            ResourceUri::File { .. } => None,
        }
    }
}

fn encode(segment: &str) -> String {
    segment.replace('%', "%25").replace('/', "%2F")
}

fn decode(segment: &str) -> ToolResult<String> {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = segment
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| ToolError::invalid(format!("bad escape in {segment:?}")))?;
            out.push(hex);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ToolError::invalid(format!("bad escape in {segment:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip_with_slashes_in_segments() {
        let change = ChangeId::new();
        for uri in [
            ResourceUri::File {
                repo: "acme/web".into(),
                root: "release/1.0".into(),
                path: RepoPathBuf::new("src/a b.rs").unwrap(),
            },
            ResourceUri::Diff {
                repo: "main".into(),
                change,
            },
            ResourceUri::Review {
                repo: "main".into(),
                change,
            },
        ] {
            assert_eq!(ResourceUri::parse(&uri.to_uri()).unwrap(), uri);
        }
    }

    #[test]
    fn rejects_foreign_and_malformed_uris() {
        for bad in [
            "file:///etc/passwd",
            "rill://",
            "rill://main/changes/ch_x/diff",
            "rill://main/files/trunk",
            "rill://main/other/x",
            "rill://m%zz/files/trunk/a",
        ] {
            assert!(ResourceUri::parse(bad).is_err(), "{bad}");
        }
    }
}

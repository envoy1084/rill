//! Limits and shape rules every stored change must respect. They keep each
//! request, transaction and materialization bounded, and keep repository
//! content from reaching outside a checkout.

use crate::error::{Error, Result};
use crate::objects::change::{ChangeVersion, FileChange, FileKind, SubtreeChange};
use crate::path::RepoPath;
use crate::Encodable;

pub const MAX_PATH_BYTES: usize = 4096;
pub const MAX_COMPONENT_BYTES: usize = 255;
pub const MAX_FILES_PER_CHANGE: usize = 100_000;
/// Encoded size of one change version.
pub const MAX_CHANGE_BYTES: usize = 8 << 20;
pub const MAX_FILE_BYTES: u64 = 5 << 30;
/// Terms in one conflict (32 sides).
pub const MAX_MERGE_TERMS: usize = 63;
pub const MAX_SYMLINK_TARGET_BYTES: usize = 4096;
/// A change message, a comment or a reason.
pub const MAX_MESSAGE_BYTES: usize = 64 << 10;
pub const MAX_EXTRA_ENTRIES: usize = 32;
pub const MAX_EXTRA_VALUE_BYTES: usize = 4096;

/// Files a checkout keeps its own state in. Committing them would let a
/// repository rewrite another user's endpoint and credentials.
pub const RESERVED_PATHS: &[&str] = &[".rill/checkout.toml", ".rill/manifest.toml"];

/// Names Windows cannot create, with or without an extension.
const WINDOWS_RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn reject(reason: impl Into<String>) -> Error {
    Error::ChangeRejected(reason.into())
}

/// Check the parts of `cv` that need no file content: counts, sizes,
/// conflict width, reserved paths, inline symlink targets and extension
/// slots. Symlink targets stored as blobs are checked by the caller, which
/// can read them.
pub fn check_change(cv: &ChangeVersion) -> Result<()> {
    let entries = cv.file_changes.len() + cv.subtree_changes.len();
    if entries > MAX_FILES_PER_CHANGE {
        return Err(reject(format!(
            "{entries} paths changed; the limit is {MAX_FILES_PER_CHANGE}"
        )));
    }
    for (path, change) in &cv.file_changes {
        match change {
            FileChange::Set { content, kind, .. } => {
                if content.size() > MAX_FILE_BYTES {
                    return Err(reject(format!("{path} is larger than 5 GiB")));
                }
                check_inline(path, content)?;
                if is_reserved(path) {
                    return Err(reject(format!("{path} is reserved for client state")));
                }
                if *kind == FileKind::Symlink {
                    if let crate::ContentRef::Inline { bytes } = content {
                        check_symlink_target(path, bytes)?;
                    }
                }
            }
            FileChange::Delete => {}
            FileChange::Conflict(terms) => {
                if terms.as_slice().len() > MAX_MERGE_TERMS {
                    return Err(reject(format!(
                        "conflict at {path} has more than {MAX_MERGE_TERMS} terms"
                    )));
                }
                for (content, _) in terms.as_slice().iter().flatten() {
                    check_inline(path, content)?;
                }
                if is_reserved(path) {
                    return Err(reject(format!("{path} is reserved for client state")));
                }
            }
        }
    }
    for (path, change) in &cv.subtree_changes {
        if matches!(change, SubtreeChange::Copy { .. }) && covers_reserved(path) {
            return Err(reject(format!(
                "copying into {path} would write reserved paths"
            )));
        }
    }
    if cv.message.len() > MAX_MESSAGE_BYTES {
        return Err(reject(format!(
            "the message is longer than {MAX_MESSAGE_BYTES} bytes"
        )));
    }
    if cv.extra.len() > MAX_EXTRA_ENTRIES {
        return Err(reject(format!(
            "more than {MAX_EXTRA_ENTRIES} extra fields"
        )));
    }
    if let Some((key, _)) = cv
        .extra
        .iter()
        .find(|(k, v)| k.len() > 64 || v.len() > MAX_EXTRA_VALUE_BYTES)
    {
        return Err(reject(format!("extra field {key:?} is too large")));
    }
    let size = cv.encode().len();
    if size > MAX_CHANGE_BYTES {
        return Err(reject(format!(
            "change encodes to {size} bytes; the limit is {MAX_CHANGE_BYTES}"
        )));
    }
    Ok(())
}

/// Inline content is for small files only; anything larger is a blob, so it
/// is stored, scanned and fetched like every other file.
fn check_inline(path: &RepoPath, content: &crate::ContentRef) -> Result<()> {
    match content {
        crate::ContentRef::Inline { bytes } if bytes.len() > crate::INLINE_THRESHOLD => {
            Err(reject(format!(
                "{path}: inline content of {} bytes exceeds {}; store it as a blob",
                bytes.len(),
                crate::INLINE_THRESHOLD
            )))
        }
        _ => Ok(()),
    }
}

/// A symlink at `link` pointing at `target` must stay inside the
/// repository and away from client state, so materializing it can never
/// write or read outside the checkout.
pub fn check_symlink_target(link: &RepoPath, target: &[u8]) -> Result<()> {
    let bad = |why: &str| reject(format!("symlink {link}: {why}"));
    if target.len() > MAX_SYMLINK_TARGET_BYTES {
        return Err(bad("target longer than 4096 bytes"));
    }
    let target = std::str::from_utf8(target).map_err(|_| bad("target is not UTF-8"))?;
    if target.is_empty() {
        return Err(bad("empty target"));
    }
    if target.starts_with('/') || target.contains('\\') || target.contains('\0') {
        return Err(bad("target must be a relative path"));
    }
    let mut resolved: Vec<&str> = link.as_str().split('/').collect();
    resolved.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if resolved.pop().is_none() {
                    return Err(bad("target escapes the repository"));
                }
            }
            name => resolved.push(name),
        }
    }
    let resolved = resolved.join("/");
    if RESERVED_PATHS.contains(&resolved.as_str()) {
        return Err(bad("target is reserved for client state"));
    }
    Ok(())
}

/// Whether a path component cannot exist on Windows.
pub fn is_windows_reserved(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    WINDOWS_RESERVED.contains(&stem.to_ascii_lowercase().as_str())
        || component.ends_with('.')
        || component.ends_with(' ')
        || component.contains([':', '*', '?', '"', '<', '>', '|'])
}

/// The key two names collide under on a case-insensitive filesystem.
pub fn case_key(name: &str) -> String {
    name.to_lowercase()
}

fn is_reserved(path: &RepoPath) -> bool {
    RESERVED_PATHS.contains(&path.as_str())
}

fn covers_reserved(dir: &RepoPath) -> bool {
    RESERVED_PATHS.iter().any(|r| {
        r.strip_prefix(dir.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RepoPathBuf;

    fn link(p: &str) -> RepoPathBuf {
        RepoPathBuf::new(p).unwrap()
    }

    #[test]
    fn symlinks_stay_inside_the_repository() {
        assert!(check_symlink_target(&link("a/b/l"), b"../c").is_ok());
        assert!(check_symlink_target(&link("a/l"), b"../top").is_ok());
        assert!(check_symlink_target(&link("a/l"), b"../../etc/passwd").is_err());
        assert!(check_symlink_target(&link("l"), b"/etc/passwd").is_err());
        assert!(check_symlink_target(&link("l"), b".rill/checkout.toml").is_err());
        assert!(check_symlink_target(&link("x/l"), b"../.rill/manifest.toml").is_err());
    }

    #[test]
    fn windows_names_are_recognised_with_extensions() {
        for name in ["CON", "nul.txt", "Com1.rs", "a:b", "trailing."] {
            assert!(is_windows_reserved(name), "{name}");
        }
        for name in ["console", "null.rs", "lpt10"] {
            assert!(!is_windows_reserved(name), "{name}");
        }
    }

    fn change(content: crate::ContentRef, message: &str) -> ChangeVersion {
        let mut cv = ChangeVersion {
            change_id: crate::ChangeId::new(),
            parents: Vec::new(),
            predecessor: None,
            subtree_changes: Default::default(),
            file_changes: Default::default(),
            author: crate::Principal::human("ann"),
            provenance: Default::default(),
            intent: None,
            message: message.into(),
            created_at: crate::Timestamp::from_millis(1),
            extra: Default::default(),
        };
        cv.file_changes
            .insert(link("a.txt"), FileChange::set(content, FileKind::Regular));
        cv
    }

    #[test]
    fn inline_content_and_messages_are_bounded() {
        let small = crate::ContentRef::inline(vec![b'x'; crate::INLINE_THRESHOLD]);
        let large = crate::ContentRef::inline(vec![b'x'; crate::INLINE_THRESHOLD + 1]);
        assert!(check_change(&change(small.clone(), "ok")).is_ok());
        assert!(check_change(&change(large, "ok")).is_err());
        assert!(check_change(&change(small, &"m".repeat(MAX_MESSAGE_BYTES + 1))).is_err());
    }
}

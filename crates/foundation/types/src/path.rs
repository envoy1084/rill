//! Repository paths.
//!
//! A `RepoPath` is a normalized, `/`-separated, relative path inside a
//! repository tree. The empty path is the root. Paths are NFC-normalized, so
//! the same name typed on macOS (NFD) and Linux has one spelling, and
//! validation rejects `.`, `..`, empty components, backslashes, control
//! characters and over-long names, so every path is safe to materialize on
//! any common filesystem and sorts the same everywhere.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use unicode_normalization::{is_nfc, UnicodeNormalization};

use crate::error::{Error, Result};
use crate::limits::{MAX_COMPONENT_BYTES, MAX_PATH_BYTES};

/// Owned repository path.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RepoPathBuf(String);

/// Borrowed repository path.
#[derive(PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct RepoPath(str);

impl RepoPathBuf {
    pub fn root() -> Self {
        RepoPathBuf(String::new())
    }

    pub fn new(path: impl AsRef<str>) -> Result<Self> {
        let raw = path.as_ref();
        let trimmed = raw.trim_matches('/');
        if trimmed.is_empty() {
            return Ok(Self::root());
        }
        let normalized = nfc(trimmed);
        validate(&normalized).map_err(|reason| Error::InvalidPath {
            path: raw.to_string(),
            reason,
        })?;
        Ok(RepoPathBuf(normalized))
    }

    pub fn as_path(&self) -> &RepoPath {
        RepoPath::from_validated(&self.0)
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn push(&mut self, component: &str) -> Result<()> {
        let component = nfc(component);
        let invalid = |reason| Error::InvalidPath {
            path: component.clone(),
            reason,
        };
        validate_component(&component).map_err(invalid)?;
        let separator = usize::from(!self.0.is_empty());
        if self.0.len() + separator + component.len() > MAX_PATH_BYTES {
            return Err(invalid("path longer than 4096 bytes"));
        }
        if separator == 1 {
            self.0.push('/');
        }
        self.0.push_str(&component);
        Ok(())
    }

    pub fn join(&self, component: &str) -> Result<RepoPathBuf> {
        let mut out = self.clone();
        out.push(component)?;
        Ok(out)
    }
}

impl RepoPath {
    fn from_validated(s: &str) -> &RepoPath {
        // SAFETY: RepoPath is a transparent wrapper over str.
        unsafe { &*(s as *const str as *const RepoPath) }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|c| !c.is_empty())
    }

    /// Last component, or `None` for the root.
    pub fn file_name(&self) -> Option<&str> {
        if self.is_root() {
            None
        } else {
            self.0.rsplit('/').next()
        }
    }

    /// Parent directory; the root's parent is the root.
    pub fn parent(&self) -> &RepoPath {
        match self.0.rfind('/') {
            Some(idx) => RepoPath::from_validated(&self.0[..idx]),
            None => RepoPath::from_validated(""),
        }
    }

    /// Returns true if `self` is `ancestor` or lies beneath it.
    pub fn starts_with(&self, ancestor: &RepoPath) -> bool {
        if ancestor.is_root() {
            return true;
        }
        self.0 == ancestor.0
            || (self.0.len() > ancestor.0.len()
                && self.0.starts_with(&ancestor.0)
                && self.0.as_bytes()[ancestor.0.len()] == b'/')
    }

    /// Path relative to `ancestor`, if `self` is beneath it.
    pub fn strip_prefix(&self, ancestor: &RepoPath) -> Option<&RepoPath> {
        if ancestor.is_root() {
            return Some(self);
        }
        if self.0 == ancestor.0 {
            return Some(RepoPath::from_validated(""));
        }
        if self.starts_with(ancestor) {
            Some(RepoPath::from_validated(&self.0[ancestor.0.len() + 1..]))
        } else {
            None
        }
    }

    pub fn extension(&self) -> Option<&str> {
        let name = self.file_name()?;
        let idx = name.rfind('.')?;
        if idx == 0 {
            None
        } else {
            Some(&name[idx + 1..])
        }
    }

    pub fn to_owned_path(&self) -> RepoPathBuf {
        RepoPathBuf(self.0.to_string())
    }
}

fn nfc(s: &str) -> String {
    if is_nfc(s) {
        s.to_string()
    } else {
        s.nfc().collect()
    }
}

fn validate(path: &str) -> std::result::Result<(), &'static str> {
    if path.len() > MAX_PATH_BYTES {
        return Err("path longer than 4096 bytes");
    }
    for component in path.split('/') {
        validate_component(component)?;
    }
    Ok(())
}

fn validate_component(component: &str) -> std::result::Result<(), &'static str> {
    if component.is_empty() {
        return Err("empty path component");
    }
    if component == "." || component == ".." {
        return Err("relative path component");
    }
    if component.contains('\\') {
        return Err("backslash in path");
    }
    if component.contains('\0') {
        return Err("NUL in path");
    }
    if component.chars().any(char::is_control) {
        return Err("control character in path");
    }
    if component.len() > MAX_COMPONENT_BYTES {
        return Err("path component longer than 255 bytes");
    }
    if component.contains('/') {
        return Err("slash in path component");
    }
    Ok(())
}

impl Deref for RepoPathBuf {
    type Target = RepoPath;

    fn deref(&self) -> &RepoPath {
        self.as_path()
    }
}

impl Borrow<RepoPath> for RepoPathBuf {
    fn borrow(&self) -> &RepoPath {
        self.as_path()
    }
}

impl AsRef<RepoPath> for RepoPathBuf {
    fn as_ref(&self) -> &RepoPath {
        self.as_path()
    }
}

impl AsRef<RepoPath> for RepoPath {
    fn as_ref(&self) -> &RepoPath {
        self
    }
}

impl ToOwned for RepoPath {
    type Owned = RepoPathBuf;

    fn to_owned(&self) -> RepoPathBuf {
        self.to_owned_path()
    }
}

impl FromStr for RepoPathBuf {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        RepoPathBuf::new(s)
    }
}

impl TryFrom<&str> for RepoPathBuf {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        RepoPathBuf::new(value)
    }
}

impl TryFrom<String> for RepoPathBuf {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        RepoPathBuf::new(value)
    }
}

impl fmt::Debug for RepoPathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl fmt::Display for RepoPathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", &self.0)
    }
}

impl fmt::Display for RepoPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_slashes() {
        let p = RepoPathBuf::new("/src/lib.rs/").unwrap();
        assert_eq!(p.as_str(), "src/lib.rs");
        assert_eq!(p.file_name(), Some("lib.rs"));
        assert_eq!(p.parent().as_str(), "src");
        assert_eq!(p.extension(), Some("rs"));
    }

    #[test]
    fn rejects_relative_components() {
        assert!(RepoPathBuf::new("a/../b").is_err());
        assert!(RepoPathBuf::new("a//b").is_err());
        assert!(RepoPathBuf::new("a\\b").is_err());
    }

    #[test]
    fn prefix_semantics() {
        let a = RepoPathBuf::new("src").unwrap();
        let b = RepoPathBuf::new("src/lib.rs").unwrap();
        let c = RepoPathBuf::new("srcx/lib.rs").unwrap();
        assert!(b.starts_with(&a));
        assert!(!c.starts_with(&a));
        assert_eq!(b.strip_prefix(&a).unwrap().as_str(), "lib.rs");
        assert!(b.starts_with(&RepoPathBuf::root()));
    }
}

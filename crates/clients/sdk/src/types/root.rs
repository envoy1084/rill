//! Points in history that tree and diff operations start from.

use std::fmt;
use std::str::FromStr;

use rill_types::{ChangeVersionHash, ObjectHash, RefName, TreeHash, TrunkSeq};

use crate::error::{Error, Result};

/// A point in history named by ref, version hash, tree hash or trunk
/// sequence number. Its display form is the revision string requests
/// carry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Root {
    /// The current target of a ref.
    Ref(RefName),
    /// A specific change version.
    Version(ChangeVersionHash),
    /// A bare tree, with no change attached.
    Tree(TreeHash),
    /// The trunk version at a sequence number.
    Seq(TrunkSeq),
}

impl Root {
    /// The head of trunk.
    pub fn trunk() -> Self {
        Root::Ref(RefName::trunk())
    }

    /// Parse a ref name into a root.
    pub fn named(name: &str) -> Result<Self> {
        Ok(Root::Ref(RefName::new(name)?))
    }
}

impl fmt::Display for Root {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Root::Ref(name) => write!(f, "{name}"),
            Root::Version(h) => write!(f, "{h}"),
            Root::Tree(h) => write!(f, "tree:{h}"),
            Root::Seq(s) => write!(f, "{s}"),
        }
    }
}

impl FromStr for Root {
    type Err = Error;

    /// `trunk@N` is a sequence, `tree:<hex>` a tree, 64 hex chars a
    /// version, anything else a ref name.
    fn from_str(s: &str) -> Result<Self> {
        if let Some(rest) = s.strip_prefix("tree:") {
            return Ok(Root::Tree(ObjectHash::from_hex(rest)?));
        }
        if s.starts_with("trunk@") {
            return Ok(Root::Seq(s.parse()?));
        }
        if s.len() == ObjectHash::LEN * 2 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(Root::Version(ObjectHash::from_hex(s)?));
        }
        Ok(Root::Ref(RefName::new(s)?))
    }
}

impl From<ChangeVersionHash> for Root {
    fn from(h: ChangeVersionHash) -> Self {
        Root::Version(h)
    }
}

impl From<TrunkSeq> for Root {
    fn from(s: TrunkSeq) -> Self {
        Root::Seq(s)
    }
}

impl From<RefName> for Root {
    fn from(r: RefName) -> Self {
        Root::Ref(r)
    }
}

/// Where a history walk begins.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LogStart {
    /// The head of trunk.
    Trunk,
    /// The target of a ref.
    Ref(RefName),
    /// A specific version.
    Version(ChangeVersionHash),
    /// A trunk sequence number.
    Seq(TrunkSeq),
}

impl LogStart {
    /// The equivalent root.
    pub fn to_root(&self) -> Root {
        match self {
            LogStart::Trunk => Root::trunk(),
            LogStart::Ref(r) => Root::Ref(r.clone()),
            LogStart::Version(h) => Root::Version(*h),
            LogStart::Seq(s) => Root::Seq(*s),
        }
    }
}

impl From<Root> for LogStart {
    /// A tree root has no history; it is mapped to trunk.
    fn from(root: Root) -> Self {
        match root {
            Root::Ref(r) if r.is_trunk() => LogStart::Trunk,
            Root::Ref(r) => LogStart::Ref(r),
            Root::Version(h) => LogStart::Version(h),
            Root::Seq(s) => LogStart::Seq(s),
            Root::Tree(_) => LogStart::Trunk,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display_roundtrip() {
        let h = ObjectHash::digest(rill_types::ObjectKind::Blob, b"x");
        for s in [
            "trunk".to_string(),
            "trunk@7".to_string(),
            h.to_hex(),
            format!("tree:{}", h.to_hex()),
            "release/1.0".to_string(),
        ] {
            let root: Root = s.parse().unwrap();
            assert_eq!(root.to_string(), s);
        }
        assert_eq!("trunk".parse::<Root>().unwrap(), Root::trunk());
        assert!(matches!(
            "trunk@3".parse::<Root>().unwrap(),
            Root::Seq(TrunkSeq(3))
        ));
    }
}

//! Named pointers and trunk sequence numbers.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::objects::change::ChangeVersionHash;

/// Position of a landed change on trunk. Monotonic, gap-free, and the
/// cheapest name for a point in history.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(transparent)]
pub struct TrunkSeq(pub u64);

impl TrunkSeq {
    pub const ZERO: TrunkSeq = TrunkSeq(0);

    pub fn next(self) -> TrunkSeq {
        TrunkSeq(self.0 + 1)
    }

    pub fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for TrunkSeq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "trunk@{}", self.0)
    }
}

impl FromStr for TrunkSeq {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let digits = s.strip_prefix("trunk@").unwrap_or(s);
        digits
            .parse::<u64>()
            .map(TrunkSeq)
            .map_err(|_| Error::InvalidId(format!("invalid trunk sequence {s:?}")))
    }
}

/// A validated ref name.
///
/// Only a few refs exist by design: `trunk`, `release/<name>`, and
/// per-principal bookmarks `<principal>/<name>`. Refs are never a
/// coordination point on the hot path.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RefName(String);

impl RefName {
    pub const TRUNK: &'static str = "trunk";

    pub fn trunk() -> Self {
        RefName(Self::TRUNK.to_string())
    }

    pub fn release(name: &str) -> Result<Self> {
        RefName::new(format!("release/{name}"))
    }

    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let valid = !name.is_empty()
            && name.len() <= 256
            && !name.starts_with('/')
            && !name.ends_with('/')
            && !name.contains("//")
            && !name.contains("..")
            && name.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.' | ':' | '@')
            });
        if valid {
            Ok(RefName(name))
        } else {
            Err(Error::InvalidRefName(name))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_trunk(&self) -> bool {
        self.0 == Self::TRUNK
    }

    pub fn is_release(&self) -> bool {
        self.0.starts_with("release/")
    }
}

impl fmt::Debug for RefName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl fmt::Display for RefName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RefName {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        RefName::new(s)
    }
}

/// A line of landings, moved only by its lander: trunk, or a release
/// ref cut from it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum Line {
    #[default]
    Trunk,
    /// `release/<name>`, by name.
    Release(String),
}

impl Line {
    /// The line a ref name moves, if it is one: `trunk` or `release/<name>`.
    pub fn of_ref(name: &RefName) -> Option<Line> {
        if name.is_trunk() {
            return Some(Line::Trunk);
        }
        name.as_str()
            .strip_prefix("release/")
            .map(|n| Line::Release(n.to_string()))
    }

    /// The ref the line moves.
    pub fn ref_name(&self) -> RefName {
        match self {
            Line::Trunk => RefName::trunk(),
            // Built from a validated release name.
            Line::Release(name) => RefName(format!("release/{name}")),
        }
    }

    pub fn is_trunk(&self) -> bool {
        *self == Line::Trunk
    }

    /// The release name, for a release line.
    pub fn release(&self) -> Option<&str> {
        match self {
            Line::Trunk => None,
            Line::Release(name) => Some(name),
        }
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.ref_name())
    }
}

/// A compare-and-swap request on a ref.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefUpdate {
    pub name: RefName,
    /// Expected current target; `None` means the ref must not exist.
    pub expected: Option<ChangeVersionHash>,
    /// New target; `None` deletes the ref.
    pub target: Option<ChangeVersionHash>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_trunk_and_release_refs_only() {
        let release = RefName::release("1.0").unwrap();
        assert_eq!(Line::of_ref(&release), Some(Line::Release("1.0".into())));
        assert_eq!(Line::of_ref(&RefName::trunk()), Some(Line::Trunk));
        assert_eq!(Line::of_ref(&RefName::new("ann/wip").unwrap()), None);
        assert_eq!(Line::Release("1.0".into()).ref_name(), release);
    }

    #[test]
    fn ref_validation() {
        assert!(RefName::new("trunk").unwrap().is_trunk());
        assert!(RefName::new("release/1.0").unwrap().is_release());
        assert!(RefName::new("agent:planner-7/stage-2").is_ok());
        assert!(RefName::new("/x").is_err());
        assert!(RefName::new("a//b").is_err());
        assert!(RefName::new("a b").is_err());
    }

    #[test]
    fn seq_parse() {
        assert_eq!("trunk@42".parse::<TrunkSeq>().unwrap(), TrunkSeq(42));
        assert_eq!("42".parse::<TrunkSeq>().unwrap(), TrunkSeq(42));
        assert_eq!(TrunkSeq(7).to_string(), "trunk@7");
    }
}

//! Ref updates.

use rill_types::RefUpdate;

use crate::convert::primitives::{hash_to_proto, opt_hash_from_proto, ref_name_from_proto};
use crate::error::{ConvertError, Result};
use crate::rill::v1 as pb;

impl From<&RefUpdate> for pb::RefUpdate {
    fn from(u: &RefUpdate) -> Self {
        pb::RefUpdate {
            name: u.name.to_string(),
            expected: u.expected.as_ref().map(hash_to_proto),
            target: u.target.as_ref().map(hash_to_proto),
        }
    }
}

impl From<RefUpdate> for pb::RefUpdate {
    fn from(u: RefUpdate) -> Self {
        pb::RefUpdate::from(&u)
    }
}

impl TryFrom<pb::RefUpdate> for RefUpdate {
    type Error = ConvertError;

    fn try_from(u: pb::RefUpdate) -> Result<Self> {
        Ok(RefUpdate {
            name: ref_name_from_proto("ref_update.name", &u.name)?,
            expected: opt_hash_from_proto("ref_update.expected", u.expected.as_ref())?,
            target: opt_hash_from_proto("ref_update.target", u.target.as_ref())?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{ObjectHash, ObjectKind, RefName};

    #[test]
    fn ref_update_roundtrip() {
        let u = RefUpdate {
            name: RefName::release("1.0").unwrap(),
            expected: None,
            target: Some(ObjectHash::digest(ObjectKind::ChangeVersion, b"v")),
        };
        assert_eq!(RefUpdate::try_from(pb::RefUpdate::from(&u)).unwrap(), u);
    }

    #[test]
    fn ref_update_rejects_bad_name() {
        let wire = pb::RefUpdate {
            name: "/bad".into(),
            expected: None,
            target: None,
        };
        assert!(matches!(
            RefUpdate::try_from(wire),
            Err(ConvertError::Invalid {
                field: "ref_update.name",
                ..
            })
        ));
    }
}

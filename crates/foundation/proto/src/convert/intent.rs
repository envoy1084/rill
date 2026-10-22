//! Intent and risk conversions.

use rill_types::{Intent, IntentKind, IntentStatus, RiskClass};

use crate::convert::primitives::{
    enum_from_proto, id_from_proto, id_to_proto, ids_from_proto, opt_id_from_proto, required,
    timestamp_from_proto, timestamp_to_proto,
};
use crate::error::{ConvertError, Result};
use crate::rill::v1 as pb;

impl From<RiskClass> for pb::RiskClass {
    fn from(r: RiskClass) -> Self {
        match r {
            RiskClass::Trivial => pb::RiskClass::Trivial,
            RiskClass::Low => pb::RiskClass::Low,
            RiskClass::Normal => pb::RiskClass::Normal,
            RiskClass::Sensitive => pb::RiskClass::Sensitive,
            RiskClass::Critical => pb::RiskClass::Critical,
        }
    }
}

impl TryFrom<pb::RiskClass> for RiskClass {
    type Error = ConvertError;

    fn try_from(r: pb::RiskClass) -> Result<Self> {
        match r {
            pb::RiskClass::Trivial => Ok(RiskClass::Trivial),
            pb::RiskClass::Low => Ok(RiskClass::Low),
            pb::RiskClass::Normal => Ok(RiskClass::Normal),
            pb::RiskClass::Sensitive => Ok(RiskClass::Sensitive),
            pb::RiskClass::Critical => Ok(RiskClass::Critical),
            pb::RiskClass::Unspecified => Err(ConvertError::BadEnum {
                field: "risk",
                value: 0,
            }),
        }
    }
}

/// Decode a `RiskClass` enum field.
pub fn risk_from_proto(field: &'static str, value: i32) -> Result<RiskClass> {
    let r: pb::RiskClass = enum_from_proto(field, value)?;
    RiskClass::try_from(r).map_err(|_| ConvertError::BadEnum { field, value })
}

impl From<IntentKind> for pb::IntentKind {
    fn from(k: IntentKind) -> Self {
        match k {
            IntentKind::Feature => pb::IntentKind::Feature,
            IntentKind::Bug => pb::IntentKind::Bug,
            IntentKind::Refactor => pb::IntentKind::Refactor,
            IntentKind::Chore => pb::IntentKind::Chore,
            IntentKind::Experiment => pb::IntentKind::Experiment,
            IntentKind::System => pb::IntentKind::System,
        }
    }
}

/// Decode an `IntentKind` enum field.
pub fn intent_kind_from_proto(field: &'static str, value: i32) -> Result<IntentKind> {
    let k: pb::IntentKind = enum_from_proto(field, value)?;
    match k {
        pb::IntentKind::Feature => Ok(IntentKind::Feature),
        pb::IntentKind::Bug => Ok(IntentKind::Bug),
        pb::IntentKind::Refactor => Ok(IntentKind::Refactor),
        pb::IntentKind::Chore => Ok(IntentKind::Chore),
        pb::IntentKind::Experiment => Ok(IntentKind::Experiment),
        pb::IntentKind::System => Ok(IntentKind::System),
        pb::IntentKind::Unspecified => Err(ConvertError::BadEnum { field, value }),
    }
}

impl From<IntentStatus> for pb::IntentStatus {
    fn from(s: IntentStatus) -> Self {
        match s {
            IntentStatus::Open => pb::IntentStatus::Open,
            IntentStatus::InProgress => pb::IntentStatus::InProgress,
            IntentStatus::Done => pb::IntentStatus::Done,
            IntentStatus::Abandoned => pb::IntentStatus::Abandoned,
        }
    }
}

/// Decode an `IntentStatus` enum field.
pub fn intent_status_from_proto(field: &'static str, value: i32) -> Result<IntentStatus> {
    let s: pb::IntentStatus = enum_from_proto(field, value)?;
    match s {
        pb::IntentStatus::Open => Ok(IntentStatus::Open),
        pb::IntentStatus::InProgress => Ok(IntentStatus::InProgress),
        pb::IntentStatus::Done => Ok(IntentStatus::Done),
        pb::IntentStatus::Abandoned => Ok(IntentStatus::Abandoned),
        pb::IntentStatus::Unspecified => Err(ConvertError::BadEnum { field, value }),
    }
}

impl From<&Intent> for pb::Intent {
    fn from(i: &Intent) -> Self {
        pb::Intent {
            id: id_to_proto(&i.id),
            title: i.title.clone(),
            body: i.body.clone(),
            kind: pb::IntentKind::from(i.kind) as i32,
            status: pb::IntentStatus::from(i.status) as i32,
            owner: Some(pb::Principal::from(&i.owner)),
            parent: i.parent.as_ref().map(id_to_proto),
            risk_override: i.risk_override.map(|r| pb::RiskClass::from(r) as i32),
            changes: i.changes.iter().map(id_to_proto).collect(),
            created_at: timestamp_to_proto(i.created_at),
            updated_at: timestamp_to_proto(i.updated_at),
        }
    }
}

impl From<Intent> for pb::Intent {
    fn from(i: Intent) -> Self {
        pb::Intent::from(&i)
    }
}

impl TryFrom<pb::Intent> for Intent {
    type Error = ConvertError;

    fn try_from(i: pb::Intent) -> Result<Self> {
        Ok(Intent {
            id: id_from_proto("intent.id", &i.id)?,
            title: i.title,
            body: i.body,
            kind: intent_kind_from_proto("intent.kind", i.kind)?,
            status: intent_status_from_proto("intent.status", i.status)?,
            owner: required("intent.owner", i.owner)?.try_into()?,
            parent: opt_id_from_proto("intent.parent", i.parent.as_ref())?,
            risk_override: i
                .risk_override
                .map(|r| risk_from_proto("intent.risk_override", r))
                .transpose()?,
            changes: ids_from_proto("intent.changes", &i.changes)?,
            created_at: timestamp_from_proto(i.created_at),
            updated_at: timestamp_from_proto(i.updated_at),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{ChangeId, IntentId, Principal, Timestamp};

    #[test]
    fn intent_roundtrip() {
        let intent = Intent {
            id: IntentId::derived("t", "i"),
            title: "Add login".into(),
            body: "Because".into(),
            kind: IntentKind::Feature,
            status: IntentStatus::InProgress,
            owner: Principal::human("alice"),
            parent: Some(IntentId::derived("t", "p")),
            risk_override: Some(RiskClass::Sensitive),
            changes: vec![ChangeId::derived("t", "a"), ChangeId::derived("t", "b")],
            created_at: Timestamp::from_millis(1),
            updated_at: Timestamp::from_millis(2),
        };
        assert_eq!(Intent::try_from(pb::Intent::from(&intent)).unwrap(), intent);

        let mut minimal = intent.clone();
        minimal.parent = None;
        minimal.risk_override = None;
        minimal.changes.clear();
        assert_eq!(
            Intent::try_from(pb::Intent::from(&minimal)).unwrap(),
            minimal
        );
    }

    #[test]
    fn risk_roundtrip_all_values() {
        for r in [
            RiskClass::Trivial,
            RiskClass::Low,
            RiskClass::Normal,
            RiskClass::Sensitive,
            RiskClass::Critical,
        ] {
            assert_eq!(
                risk_from_proto("r", pb::RiskClass::from(r) as i32).unwrap(),
                r
            );
        }
        assert!(risk_from_proto("r", 0).is_err());
    }
}

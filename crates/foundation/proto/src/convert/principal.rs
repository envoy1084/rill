//! Principal and provenance conversions.

use rill_types::{Principal, PrincipalKind, Provenance};

use crate::convert::primitives::{enum_from_proto, id_from_proto, id_to_proto};
use crate::error::{ConvertError, Result};
use crate::rill::v1 as pb;

impl From<PrincipalKind> for pb::PrincipalKind {
    fn from(kind: PrincipalKind) -> Self {
        match kind {
            PrincipalKind::Human => pb::PrincipalKind::Human,
            PrincipalKind::Agent => pb::PrincipalKind::Agent,
            PrincipalKind::Service => pb::PrincipalKind::Service,
        }
    }
}

impl TryFrom<pb::PrincipalKind> for PrincipalKind {
    type Error = ConvertError;

    fn try_from(kind: pb::PrincipalKind) -> Result<Self> {
        match kind {
            pb::PrincipalKind::Human => Ok(PrincipalKind::Human),
            pb::PrincipalKind::Agent => Ok(PrincipalKind::Agent),
            pb::PrincipalKind::Service => Ok(PrincipalKind::Service),
            pb::PrincipalKind::Unspecified => Err(ConvertError::BadEnum {
                field: "principal.kind",
                value: 0,
            }),
        }
    }
}

impl From<&Principal> for pb::Principal {
    fn from(p: &Principal) -> Self {
        pb::Principal {
            id: id_to_proto(&p.id),
            kind: pb::PrincipalKind::from(p.kind).into(),
            name: p.name.clone(),
            identity: p.identity.clone(),
        }
    }
}

impl From<Principal> for pb::Principal {
    fn from(p: Principal) -> Self {
        pb::Principal::from(&p)
    }
}

impl TryFrom<pb::Principal> for Principal {
    type Error = ConvertError;

    fn try_from(p: pb::Principal) -> Result<Self> {
        let kind: pb::PrincipalKind = enum_from_proto("principal.kind", p.kind)?;
        Ok(Principal {
            id: id_from_proto("principal.id", &p.id)?,
            kind: kind.try_into()?,
            name: p.name,
            identity: p.identity,
        })
    }
}

impl From<&Provenance> for pb::Provenance {
    fn from(p: &Provenance) -> Self {
        pb::Provenance {
            tool: p.tool.clone(),
            model: p.model.clone(),
            session: p.session.clone(),
            prompt_ref: p.prompt_ref.clone(),
            sponsor: p.sponsor.as_ref().map(pb::Principal::from).into(),
            resolved_by: p.resolved_by.clone(),
            extra: p.extra.clone(),
        }
    }
}

impl From<Provenance> for pb::Provenance {
    fn from(p: Provenance) -> Self {
        pb::Provenance::from(&p)
    }
}

impl TryFrom<pb::Provenance> for Provenance {
    type Error = ConvertError;

    fn try_from(p: pb::Provenance) -> Result<Self> {
        Ok(Provenance {
            tool: p.tool,
            model: p.model,
            session: p.session,
            prompt_ref: p.prompt_ref,
            sponsor: p.sponsor.map(Principal::try_from).transpose()?,
            resolved_by: p.resolved_by,
            extra: p.extra,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_roundtrip() {
        let mut p = Principal::agent("reviewer-7");
        p.identity = Some("spiffe://rill/agent/reviewer-7".into());
        let wire = pb::Principal::from(&p);
        assert_eq!(Principal::try_from(wire).unwrap(), p);
    }

    #[test]
    fn principal_rejects_unspecified_kind() {
        let mut wire = pb::Principal::from(Principal::human("a"));
        wire.kind = 0.into();
        assert!(matches!(
            Principal::try_from(wire),
            Err(ConvertError::BadEnum {
                field: "principal.kind",
                ..
            })
        ));
    }

    #[test]
    fn provenance_roundtrip() {
        let mut p = Provenance {
            tool: Some("claude-code".into()),
            model: Some("m".into()),
            session: None,
            prompt_ref: Some("p".into()),
            sponsor: Some(Principal::human("alice")),
            resolved_by: None,
            extra: Default::default(),
        };
        p.extra.insert("k".into(), "v".into());
        let wire = pb::Provenance::from(&p);
        assert_eq!(Provenance::try_from(wire).unwrap(), p);
    }
}

//! Provenance the server vouches for. A version's session, model and
//! sponsor come from the signed token that wrote it; whatever the client
//! said about them is kept, marked as a claim.

use rill_policy::AuthContext;
use rill_types::{ChangeVersion, Principal, Provenance};

/// Extra keys holding client claims the token did not confirm.
pub const CLAIMED_SESSION: &str = "claimed_session";
pub const CLAIMED_MODEL: &str = "claimed_model";
/// The team sponsoring an agent, when a team does.
pub const SPONSOR_TEAM: &str = "sponsor_team";

/// Replace `field` with the token's value, keeping a different client
/// value under `claim_key`.
fn vouch(
    field: &mut Option<String>,
    token: Option<String>,
    claim_key: &str,
    extra: &mut std::collections::BTreeMap<String, String>,
) {
    let claimed = field.take();
    if let Some(claimed) = claimed.filter(|c| Some(c) != token.as_ref()) {
        extra.insert(claim_key.to_string(), claimed);
    }
    *field = token;
}

/// Stamp `cv`'s provenance from the caller's token.
pub fn stamp_provenance(cv: &mut ChangeVersion, ctx: &AuthContext) {
    let claims = ctx.claims.as_ref();
    let p: &mut Provenance = &mut cv.provenance;
    vouch(
        &mut p.session,
        claims.and_then(|c| c.session).map(|s| s.to_string()),
        CLAIMED_SESSION,
        &mut p.extra,
    );
    vouch(
        &mut p.model,
        claims.and_then(|c| c.model.clone()),
        CLAIMED_MODEL,
        &mut p.extra,
    );
    p.extra.remove(SPONSOR_TEAM);
    p.sponsor = None;
    match claims
        .and_then(|c| c.sponsor.as_deref())
        .and_then(|s| s.split_once(':'))
    {
        Some(("user", name)) => p.sponsor = Some(Principal::human(name)),
        Some(("team", name)) => {
            p.extra.insert(SPONSOR_TEAM.into(), name.into());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use rill_policy::TokenClaims;
    use rill_types::{ChangeId, SessionId, Timestamp};

    use super::*;

    fn version(provenance: Provenance) -> ChangeVersion {
        ChangeVersion {
            change_id: ChangeId::new(),
            parents: vec![],
            predecessor: None,
            subtree_changes: Default::default(),
            file_changes: Default::default(),
            author: Principal::agent("bot"),
            provenance,
            intent: None,
            message: "m".into(),
            created_at: Timestamp::from_millis(1),
            extra: Default::default(),
        }
    }

    fn ctx(claims: Option<TokenClaims>) -> AuthContext {
        AuthContext {
            principal: Principal::agent("bot"),
            teams: vec![],
            capability: None,
            claims,
            scope: None,
            relations_seen: Vec::new(),
        }
    }

    #[test]
    fn the_token_not_the_client_says_session_model_and_sponsor() {
        let session = SessionId::new();
        let claims = TokenClaims::new("default", Timestamp::from_millis(1), 60_000)
            .with_session(session)
            .with_model("model-a")
            .with_sponsor("user:ann");
        let mut cv = version(Provenance {
            session: Some("made-up".into()),
            model: Some("model-a".into()),
            sponsor: Some(Principal::human("mallory")),
            ..Provenance::default()
        });
        stamp_provenance(&mut cv, &ctx(Some(claims)));
        let p = &cv.provenance;
        assert_eq!(p.session.as_deref(), Some(session.to_string().as_str()));
        assert_eq!(p.model.as_deref(), Some("model-a"));
        assert_eq!(p.sponsor, Some(Principal::human("ann")));
        assert_eq!(
            p.extra.get(CLAIMED_SESSION).map(String::as_str),
            Some("made-up")
        );
        assert!(
            !p.extra.contains_key(CLAIMED_MODEL),
            "a matching claim is not repeated"
        );
    }

    #[test]
    fn without_a_token_client_values_stay_claims() {
        let mut cv = version(Provenance {
            session: Some("s".into()),
            model: Some("m".into()),
            ..Provenance::default()
        });
        stamp_provenance(&mut cv, &ctx(None));
        assert_eq!(cv.provenance.session, None);
        assert_eq!(cv.provenance.model, None);
        assert_eq!(cv.provenance.extra[CLAIMED_SESSION], "s");
        assert_eq!(cv.provenance.extra[CLAIMED_MODEL], "m");

        let team = TokenClaims::new("default", Timestamp::from_millis(1), 60_000)
            .with_sponsor("team:infra");
        let mut cv = version(Provenance::default());
        stamp_provenance(&mut cv, &ctx(Some(team)));
        assert_eq!(cv.provenance.sponsor, None);
        assert_eq!(cv.provenance.extra[SPONSOR_TEAM], "infra");
    }
}

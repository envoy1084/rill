use rill_types::{Principal, RepoId, RepoPathBuf, Timestamp, WorkspaceId};

use super::*;
use crate::error::Error;

fn p(s: &str) -> RepoPathBuf {
    RepoPathBuf::new(s).unwrap()
}

fn repo() -> RepoId {
    RepoId::derived("t", "repo")
}

const NOW: Timestamp = Timestamp::from_millis(1_000_000_000_000);
const LATER: Timestamp = Timestamp::from_millis(1_000_000_000_000 + 900_000);

fn cap() -> Capability {
    Capability::repo_wide(repo(), [Verb::Read, Verb::Write], LATER)
}

fn claims() -> TokenClaims {
    TokenClaims::new("default", NOW, LATER.millis() - NOW.millis())
}

fn issue(issuer: &Issuer, who: &Principal, cap: &Capability) -> String {
    issuer.issue(who, &claims(), Some(cap)).unwrap()
}

fn request(path: &str, verb: Verb) -> TokenRequest {
    TokenRequest {
        repo: repo(),
        workspace: None,
        path: p(path),
        verb,
        now: NOW,
    }
}

#[test]
fn issue_then_verify() {
    let issuer = Issuer::generate();
    let verifier = Verifier::new(issuer.key_ring());
    let bot = Principal::agent("coder-7");
    let token = issue(&issuer, &bot, &cap());

    let who = verifier
        .verify(&token, &request("src/lib.rs", Verb::Read))
        .unwrap();
    assert_eq!(who, bot);
    let full = verifier
        .verify_full(&token, &request("", Verb::Write))
        .unwrap();
    assert_eq!(full.capability, Some(cap()));
    assert_eq!(full.claims, claims_of(&token, &issuer));

    assert!(matches!(
        verifier.verify(&token, &request("src/lib.rs", Verb::Land)),
        Err(Error::TokenRejected(_))
    ));
    let other_repo = TokenRequest {
        repo: RepoId::derived("t", "other"),
        ..request("x", Verb::Read)
    };
    assert!(matches!(
        verifier.verify(&token, &other_repo),
        Err(Error::TokenRejected(_))
    ));
}

fn claims_of(token: &str, issuer: &Issuer) -> TokenClaims {
    inspect(token, &issuer.key_ring()).unwrap().0
}

#[test]
fn unnarrowed_tokens_carry_identity_and_claims_only() {
    let issuer = Issuer::generate();
    let verifier = Verifier::new(issuer.key_ring());
    let alice = Principal::human("alice");
    let session = rill_types::SessionId::new();
    let claims = claims().with_session(session);
    let token = issuer.issue(&alice, &claims, None).unwrap();
    let verified = verifier.authenticate(&token, NOW).unwrap();
    assert_eq!(verified.principal, alice);
    assert_eq!(verified.capability, None);
    assert_eq!(verified.claims, claims);
    // Any repository, path and verb passes the token; relations decide.
    assert!(verifier
        .verify(&token, &request("any/where", Verb::Admin))
        .is_ok());

    // The holder can still narrow it offline, but not past its expiry.
    let narrow = Capability::repo_wide(repo(), [Verb::Read], LATER);
    let child = attenuate(&token, &issuer.key_ring(), &narrow).unwrap();
    assert_eq!(
        verifier.authenticate(&child, NOW).unwrap().capability,
        Some(narrow)
    );
    assert!(verifier.verify(&child, &request("x", Verb::Write)).is_err());
    let longer = Capability::repo_wide(
        repo(),
        [Verb::Read],
        Timestamp::from_millis(LATER.millis() + 1),
    );
    assert!(matches!(
        attenuate(&token, &issuer.key_ring(), &longer),
        Err(Error::Widening(_))
    ));
}

#[test]
fn lifetimes_are_capped_per_principal_kind() {
    let issuer = Issuer::generate();
    let hour = TokenClaims::new("default", NOW, 3_600_000);
    assert!(issuer
        .issue(&Principal::human("alice"), &hour, None)
        .is_ok());
    assert!(issuer.issue(&Principal::agent("bot"), &hour, None).is_err());
    let day = TokenClaims::new("default", NOW, 86_400_000);
    assert!(issuer.issue(&Principal::service("ci"), &day, None).is_err());
    let backwards = TokenClaims::new("default", NOW, -1);
    assert!(issuer
        .issue(&Principal::human("alice"), &backwards, None)
        .is_err());
}

#[test]
fn tokens_verify_against_the_key_that_signed_them() {
    let old = Issuer::new(SigningKey::generate(1), TokenLifetimes::default());
    let new = Issuer::new(SigningKey::generate(2), TokenLifetimes::default());
    let mut ring = KeyRing::new();
    ring.insert(1, old.public_key(), Some(LATER));
    ring.insert(2, new.public_key(), None);
    let verifier = Verifier::new(ring);
    let alice = Principal::human("alice");
    let from_old = old.issue(&alice, &claims(), None).unwrap();
    let from_new = new.issue(&alice, &claims(), None).unwrap();
    assert!(verifier.authenticate(&from_old, NOW).is_ok());
    assert!(verifier.authenticate(&from_new, NOW).is_ok());

    // A retired key no longer verifies, even for an unexpired token.
    assert!(matches!(
        verifier.authenticate(&from_old, LATER),
        Err(Error::Token(_))
    ));
    // A key the ring does not know is refused.
    let stranger = Issuer::new(SigningKey::generate(3), TokenLifetimes::default());
    let token = stranger.issue(&alice, &claims(), None).unwrap();
    assert!(matches!(
        verifier.authenticate(&token, NOW),
        Err(Error::Token(_))
    ));
    // A key reusing a trusted id with different material fails the signature.
    let impostor = Issuer::new(SigningKey::generate(2), TokenLifetimes::default());
    let token = impostor.issue(&alice, &claims(), None).unwrap();
    assert!(matches!(
        verifier.authenticate(&token, NOW),
        Err(Error::Token(_))
    ));
}

#[test]
fn expired_fails() {
    let issuer = Issuer::generate();
    let verifier = Verifier::new(issuer.key_ring());
    let token = issue(&issuer, &Principal::human("alice"), &cap());
    let late = TokenRequest {
        now: LATER,
        ..request("x", Verb::Read)
    };
    assert!(matches!(
        verifier.verify(&token, &late),
        Err(Error::TokenExpired { .. })
    ));
    let unnarrowed = issuer
        .issue(&Principal::human("alice"), &claims(), None)
        .unwrap();
    assert!(matches!(
        verifier.authenticate(&unnarrowed, LATER),
        Err(Error::TokenExpired { .. })
    ));
}

#[test]
fn attenuation_narrows() {
    let issuer = Issuer::generate();
    let ring = issuer.key_ring();
    let verifier = Verifier::new(ring.clone());
    let bot = Principal::agent("coder-7");
    let token = issue(&issuer, &bot, &cap());

    let narrower = Capability {
        verbs: vec![Verb::Read],
        paths: vec![p("src/billing"), p("docs")],
        expires_at: Timestamp::from_millis(NOW.millis() + 60_000),
        ..cap()
    };
    let child = attenuate(&token, &ring, &narrower).unwrap();
    assert_eq!(inspect(&child, &ring).unwrap().1, Some(narrower.clone()));

    let ok = |path: &str, verb| verifier.verify(&child, &request(path, verb));
    assert_eq!(ok("src/billing/x.rs", Verb::Read).unwrap(), bot);
    assert_eq!(ok("src/billing", Verb::Read).unwrap(), bot);
    assert_eq!(ok("docs/a/b.md", Verb::Read).unwrap(), bot);
    assert!(ok("src/billing/x.rs", Verb::Write).is_err());
    assert!(ok("src/auth/x.rs", Verb::Read).is_err());
    assert!(ok("src/billingx", Verb::Read).is_err());
    assert!(ok("", Verb::Read).is_err());
    // The parent still works for what the child lost.
    assert!(verifier
        .verify(&token, &request("src/auth/x.rs", Verb::Write))
        .is_ok());
    // Child expiry is enforced even though the parent is still valid.
    let late = TokenRequest {
        now: Timestamp::from_millis(NOW.millis() + 120_000),
        ..request("docs/x", Verb::Read)
    };
    assert!(matches!(
        verifier.verify(&child, &late),
        Err(Error::TokenExpired { .. })
    ));

    // A grandchild narrows further and keeps the token id.
    let grandchild_cap = Capability {
        paths: vec![p("docs")],
        ..narrower.clone()
    };
    let grandchild = attenuate(&child, &ring, &grandchild_cap).unwrap();
    assert!(verifier
        .verify(&grandchild, &request("docs/x", Verb::Read))
        .is_ok());
    assert!(verifier
        .verify(&grandchild, &request("src/billing/x", Verb::Read))
        .is_err());
    assert_eq!(
        claims_of(&grandchild, &issuer).token_id,
        claims_of(&token, &issuer).token_id
    );
}

#[test]
fn widening_attempt_fails() {
    let issuer = Issuer::generate();
    let ring = issuer.key_ring();
    let narrow = Capability {
        verbs: vec![Verb::Read],
        paths: vec![p("docs")],
        ..cap()
    };
    let token = issue(&issuer, &Principal::agent("bot"), &narrow);

    let wider_verbs = Capability {
        verbs: vec![Verb::Read, Verb::Land],
        ..narrow.clone()
    };
    assert!(matches!(
        attenuate(&token, &ring, &wider_verbs),
        Err(Error::Widening(_))
    ));
    let wider_paths = Capability {
        paths: vec![RepoPathBuf::root()],
        ..narrow.clone()
    };
    assert!(matches!(
        attenuate(&token, &ring, &wider_paths),
        Err(Error::Widening(_))
    ));
    let longer = Capability {
        expires_at: Timestamp::from_millis(LATER.millis() + 1),
        ..narrow.clone()
    };
    assert!(matches!(
        attenuate(&token, &ring, &longer),
        Err(Error::Widening(_))
    ));
}

#[test]
fn forged_attenuation_cannot_widen() {
    // Bypass `attenuate` and append a block claiming more; the parent's
    // checks still apply, so verification fails for the extra rights, and
    // the context mismatch is caught before any Datalog runs.
    let issuer = Issuer::generate();
    let verifier = Verifier::new(issuer.key_ring());
    let narrow = Capability {
        verbs: vec![Verb::Read],
        ..cap()
    };
    let token = issue(&issuer, &Principal::agent("bot"), &narrow);
    let wide = Capability {
        verbs: vec![Verb::Read, Verb::Admin],
        ..narrow.clone()
    };
    let parsed = biscuit_auth::Biscuit::from_base64(&token, |_| Ok(issuer.public_key())).unwrap();
    let forged = parsed
        .append(super::datalog::capability_block(&wide).unwrap())
        .unwrap()
        .to_base64()
        .unwrap();
    assert!(matches!(
        verifier.verify(&forged, &request("x", Verb::Admin)),
        Err(Error::Widening(_))
    ));

    // Even a block with a forged principal and token id cannot change who
    // the token belongs to, which token it is, or what it allows.
    let forged = parsed
        .append(
            biscuit_auth::builder::BlockBuilder::new()
                .code(
                    r#"principal("pr_00000000000000000000000000000000", "human", "root");
                       token("tk_00000000000000000000000000000000");"#,
                )
                .unwrap()
                .context(serde_json::to_string(&narrow).unwrap()),
        )
        .unwrap()
        .to_base64()
        .unwrap();
    let verified = verifier
        .verify_full(&forged, &request("x", Verb::Read))
        .unwrap();
    assert_eq!(verified.principal, Principal::agent("bot"));
    assert_eq!(
        verified.claims.token_id,
        claims_of(&token, &issuer).token_id
    );
    assert!(verifier
        .verify(&forged, &request("x", Verb::Admin))
        .is_err());
}

#[test]
fn workspace_pinning() {
    let issuer = Issuer::generate();
    let verifier = Verifier::new(issuer.key_ring());
    let ws = WorkspaceId::derived("t", "ws");
    let token = issue(&issuer, &Principal::agent("bot"), &cap().with_workspace(ws));
    let with_ws = TokenRequest {
        workspace: Some(ws),
        ..request("x", Verb::Write)
    };
    assert!(verifier.verify(&token, &with_ws).is_ok());
    assert!(verifier.verify(&token, &request("x", Verb::Write)).is_err());
    let other = TokenRequest {
        workspace: Some(WorkspaceId::derived("t", "other")),
        ..request("x", Verb::Write)
    };
    assert!(verifier.verify(&token, &other).is_err());
}

#[test]
fn key_roundtrip() {
    let key = SigningKey::generate(7);
    let again = SigningKey::from_secret_bytes(7, &key.secret_bytes()).unwrap();
    assert_eq!(again.public_key_bytes(), key.public_key_bytes());
    let mut ring = KeyRing::new();
    ring.insert_bytes(7, &key.public_key_bytes(), None).unwrap();
    let issuer = Issuer::new(again, TokenLifetimes::default());
    let token = issue(&issuer, &Principal::human("a"), &cap());
    assert!(Verifier::new(ring)
        .verify(&token, &request("x", Verb::Read))
        .is_ok());
    assert!(SigningKey::from_secret_bytes(1, &[1, 2, 3]).is_err());
}

#[test]
fn empty_capability_is_rejected() {
    let issuer = Issuer::generate();
    let no_verbs = Capability {
        verbs: vec![],
        ..cap()
    };
    assert!(issuer
        .issue(&Principal::human("a"), &claims(), Some(&no_verbs))
        .is_err());
}

#[test]
fn every_key_in_a_token_is_p256() {
    use biscuit_auth::format::schema::public_key::Algorithm;
    let issuer = Issuer::generate();
    let token = issue(&issuer, &Principal::agent("bot"), &cap());
    let narrower = Capability::repo_wide(repo(), [Verb::Read], LATER);
    let child = attenuate(&token, &issuer.key_ring(), &narrower).unwrap();
    let parsed = biscuit_auth::Biscuit::from_base64(&child, |_| Ok(issuer.public_key())).unwrap();
    let proto = parsed.container().to_proto();
    let p256 = Algorithm::Secp256r1 as i32;
    assert_eq!(issuer.public_key().to_proto().algorithm, p256);
    assert_eq!(proto.authority.next_key.algorithm, p256);
    assert_eq!(proto.blocks.len(), 1);
    for block in &proto.blocks {
        assert_eq!(block.next_key.algorithm, p256);
    }
}

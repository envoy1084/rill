//! Attenuated tokens grant strictly less than their parent.

use rill_types::{Principal, RepoPathBuf, Timestamp};

use super::fixtures::{cap, claims_of, issue, p, request, LATER, NOW};
use crate::error::Error;
use crate::tokens::{attenuate, inspect, Capability, Issuer, TokenRequest, Verb, Verifier};

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
        .append(crate::tokens::datalog::capability_block(&wide).unwrap())
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

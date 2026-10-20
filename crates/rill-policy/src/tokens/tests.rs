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
const LATER: Timestamp = Timestamp::from_millis(1_000_000_000_000 + 3_600_000);

fn cap() -> Capability {
    Capability::repo_wide(repo(), [Verb::Read, Verb::Write], LATER)
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
    let verifier = Verifier::new(issuer.public_key());
    let bot = Principal::agent("coder-7");
    let token = issuer.issue(&bot, &cap()).unwrap();

    let who = verifier
        .verify(&token, &request("src/lib.rs", Verb::Read))
        .unwrap();
    assert_eq!(who, bot);
    let full = verifier
        .verify_full(&token, &request("", Verb::Write))
        .unwrap();
    assert_eq!(full.capability, cap());

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

#[test]
fn wrong_root_key_fails() {
    let issuer = Issuer::generate();
    let token = issuer.issue(&Principal::human("alice"), &cap()).unwrap();
    let other = Verifier::new(Issuer::generate().public_key());
    assert!(matches!(
        other.verify(&token, &request("x", Verb::Read)),
        Err(Error::Token(_))
    ));
    assert!(matches!(
        other.verify("not a token", &request("x", Verb::Read)),
        Err(Error::Token(_))
    ));
}

#[test]
fn expired_fails() {
    let issuer = Issuer::generate();
    let verifier = Verifier::new(issuer.public_key());
    let token = issuer.issue(&Principal::human("alice"), &cap()).unwrap();
    let late = TokenRequest {
        now: Timestamp::from_millis(LATER.millis() + 1),
        ..request("x", Verb::Read)
    };
    assert!(matches!(
        verifier.verify(&token, &late),
        Err(Error::TokenExpired { .. })
    ));
}

#[test]
fn attenuation_narrows() {
    let issuer = Issuer::generate();
    let root = issuer.public_key();
    let verifier = Verifier::new(root);
    let bot = Principal::agent("coder-7");
    let token = issuer.issue(&bot, &cap()).unwrap();

    let narrower = Capability {
        verbs: vec![Verb::Read],
        paths: vec![p("src/billing"), p("docs")],
        expires_at: Timestamp::from_millis(NOW.millis() + 60_000),
        ..cap()
    };
    let child = attenuate(&token, &root, &narrower).unwrap();
    assert_eq!(inspect(&child, &root).unwrap(), narrower);

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

    // A grandchild narrows further.
    let grandchild_cap = Capability {
        paths: vec![p("docs")],
        ..narrower.clone()
    };
    let grandchild = attenuate(&child, &root, &grandchild_cap).unwrap();
    assert!(verifier
        .verify(&grandchild, &request("docs/x", Verb::Read))
        .is_ok());
    assert!(verifier
        .verify(&grandchild, &request("src/billing/x", Verb::Read))
        .is_err());
}

#[test]
fn widening_attempt_fails() {
    let issuer = Issuer::generate();
    let root = issuer.public_key();
    let narrow = Capability {
        verbs: vec![Verb::Read],
        paths: vec![p("docs")],
        ..cap()
    };
    let token = issuer.issue(&Principal::agent("bot"), &narrow).unwrap();

    let wider_verbs = Capability {
        verbs: vec![Verb::Read, Verb::Land],
        ..narrow.clone()
    };
    assert!(matches!(
        attenuate(&token, &root, &wider_verbs),
        Err(Error::Widening(_))
    ));
    let wider_paths = Capability {
        paths: vec![RepoPathBuf::root()],
        ..narrow.clone()
    };
    assert!(matches!(
        attenuate(&token, &root, &wider_paths),
        Err(Error::Widening(_))
    ));
    let longer = Capability {
        expires_at: Timestamp::from_millis(LATER.millis() + 1),
        ..narrow.clone()
    };
    assert!(matches!(
        attenuate(&token, &root, &longer),
        Err(Error::Widening(_))
    ));
}

#[test]
fn forged_attenuation_cannot_widen() {
    // Bypass `attenuate` and append a block claiming more; the parent's
    // checks still apply, so verification fails for the extra rights, and
    // the context mismatch is caught before any Datalog runs.
    let issuer = Issuer::generate();
    let root = issuer.public_key();
    let verifier = Verifier::new(root);
    let narrow = Capability {
        verbs: vec![Verb::Read],
        ..cap()
    };
    let token = issuer.issue(&Principal::agent("bot"), &narrow).unwrap();
    let wide = Capability {
        verbs: vec![Verb::Read, Verb::Admin],
        ..narrow
    };
    let parsed = biscuit_auth::Biscuit::from_base64(&token, root).unwrap();
    let forged = parsed
        .append(super::datalog::capability_block(&wide).unwrap())
        .unwrap()
        .to_base64()
        .unwrap();
    assert!(matches!(
        verifier.verify(&forged, &request("x", Verb::Admin)),
        Err(Error::Widening(_))
    ));

    // Even a block with no capability context and a forged principal fact
    // cannot change who the token belongs to or what it allows.
    let forged = parsed
        .append(
            biscuit_auth::builder::BlockBuilder::new()
                .code(r#"principal("pr_00000000000000000000000000000000", "human", "root");"#)
                .unwrap()
                .context(serde_json::to_string(&inspect(&token, &root).unwrap()).unwrap()),
        )
        .unwrap()
        .to_base64()
        .unwrap();
    let who = verifier.verify(&forged, &request("x", Verb::Read)).unwrap();
    assert_eq!(who, Principal::agent("bot"));
    assert!(verifier
        .verify(&forged, &request("x", Verb::Admin))
        .is_err());
}

#[test]
fn workspace_pinning() {
    let issuer = Issuer::generate();
    let root = issuer.public_key();
    let verifier = Verifier::new(root);
    let ws = WorkspaceId::derived("t", "ws");
    let token = issuer
        .issue(&Principal::agent("bot"), &cap().with_workspace(ws))
        .unwrap();
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
    let issuer = Issuer::generate();
    let again = Issuer::from_private_key_bytes(&issuer.private_key_bytes()).unwrap();
    assert_eq!(
        again.public_key().to_bytes(),
        issuer.public_key().to_bytes()
    );
    let v = Verifier::from_public_key_bytes(&issuer.public_key().to_bytes()).unwrap();
    let token = issuer.issue(&Principal::human("a"), &cap()).unwrap();
    assert!(v.verify(&token, &request("x", Verb::Read)).is_ok());
    assert!(Issuer::from_private_key_bytes(&[1, 2, 3]).is_err());
}

#[test]
fn empty_capability_is_rejected() {
    let issuer = Issuer::generate();
    let no_verbs = Capability {
        verbs: vec![],
        ..cap()
    };
    assert!(issuer.issue(&Principal::human("a"), &no_verbs).is_err());
}

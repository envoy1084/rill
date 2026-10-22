//! Issuing and verifying tokens: claims, lifetimes, expiry and pinning.

use rill_types::{Principal, RepoId, Timestamp, WorkspaceId};

use super::fixtures::{cap, claims, claims_of, issue, repo, request, LATER, NOW};
use crate::error::Error;
use crate::tokens::{attenuate, Capability, Issuer, TokenClaims, TokenRequest, Verb, Verifier};

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

/// A valid token must verify on a machine with no CPU to spare: Biscuit's
/// default allows 1 ms of wall-clock time per authorization, which a
/// preempted thread exceeds, and the server refused valid tokens under
/// load.
#[test]
fn tokens_verify_on_a_busy_machine() {
    let issuer = Issuer::generate();
    let verifier = Verifier::new(issuer.key_ring());
    let token = issue(&issuer, &Principal::agent("bot"), &cap());
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let threads = std::thread::available_parallelism().map_or(8, |n| n.get()) * 4;
    let spinners: Vec<_> = (0..threads)
        .map(|_| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut x = 0u64;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    x = std::hint::black_box(x.wrapping_mul(31).wrapping_add(7));
                }
            })
        })
        .collect();
    let verifiers: Vec<_> = (0..threads)
        .map(|_| {
            let (verifier, token) = (verifier.clone(), token.clone());
            std::thread::spawn(move || {
                (0..300)
                    .filter(|_| {
                        verifier
                            .verify(&token, &request("src/a", Verb::Read))
                            .is_err()
                    })
                    .count()
            })
        })
        .collect();
    let refused: usize = verifiers.into_iter().map(|t| t.join().unwrap()).sum();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for s in spinners {
        s.join().unwrap();
    }
    assert_eq!(refused, 0, "valid tokens were refused under load");
}

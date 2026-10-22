//! Shared repository, clock, capability and token builders.

use rill_types::{Principal, RepoId, RepoPathBuf, Timestamp};

use crate::tokens::{inspect, Capability, Issuer, TokenClaims, TokenRequest, Verb};

pub(super) fn p(s: &str) -> RepoPathBuf {
    RepoPathBuf::new(s).unwrap()
}

pub(super) fn repo() -> RepoId {
    RepoId::derived("t", "repo")
}

pub(super) const NOW: Timestamp = Timestamp::from_millis(1_000_000_000_000);
pub(super) const LATER: Timestamp = Timestamp::from_millis(1_000_000_000_000 + 900_000);

pub(super) fn cap() -> Capability {
    Capability::repo_wide(repo(), [Verb::Read, Verb::Write], LATER)
}

pub(super) fn claims() -> TokenClaims {
    TokenClaims::new("default", NOW, LATER.millis() - NOW.millis())
}

pub(super) fn issue(issuer: &Issuer, who: &Principal, cap: &Capability) -> String {
    issuer.issue(who, &claims(), Some(cap)).unwrap()
}

pub(super) fn request(path: &str, verb: Verb) -> TokenRequest {
    TokenRequest {
        repo: repo(),
        workspace: None,
        path: p(path),
        verb,
        now: NOW,
    }
}

pub(super) fn claims_of(token: &str, issuer: &Issuer) -> TokenClaims {
    inspect(token, &issuer.key_ring()).unwrap().0
}

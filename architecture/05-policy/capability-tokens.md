# Capability tokens

A capability token is a [Biscuit](https://www.biscuitsec.org) signed by the
instance's root Ed25519 key. Its authority block names the principal and holds
Datalog checks for one `Capability`. A holder can append blocks offline to
narrow it. The invariant: checks accumulate across blocks, so no block can
grant more than the one before it, and the principal is read only from the
authority block.

## Capability

[`Capability`](../../crates/rill-policy/src/tokens/capability.rs) is the scope
of one token:

| Field | Semantics |
| --- | --- |
| `repo: RepoId` | The single repository the token applies to. |
| `workspace: Option<WorkspaceId>` | When set, the Datalog check requires a matching `request_workspace`. |
| `paths: Vec<RepoPathBuf>` | Prefixes covered. The root covers everything. Empty fails `validate`. |
| `verbs: Vec<Verb>` | `Read`, `Write`, `Land`, `Review`, `Admin` (wire names lowercase). |
| `expires_at: Timestamp` | `is_expired_at(now)` is `now >= expires_at`. |

`check_narrower_than(parent)` returns `Error::Widening` unless all of these
hold:

* same `repo`;
* same workspace, or newly pinned when the parent has none (a pinned parent
  cannot be unpinned);
* verbs a subset of the parent's;
* every path under one of the parent's prefixes (`starts_with` by component, so
  `srcx` is not under `src`);
* `expires_at` no later than the parent's.

## Datalog

[`datalog.rs`](../../crates/rill-policy/src/tokens/datalog.rs) turns a
capability into one block. Every block carries its checks plus the
capability's JSON as the block context, so a token's grant can be read back
without re-deriving it. The authority block also holds the identity fact:

```datalog
principal("pr_…", "agent", "reviewer-7");
check if request_repo("rp_…");
check if request_workspace("ws_…");          // only when pinned
check if request_verb($v), {"read", "write"}.contains($v);
check if request_path($p), $p == "src" || $p.starts_with("src/");
check if time($t), $t < 2026-10-21T00:00:00Z;
```

A root prefix omits the `request_path` check. Expiry is rounded down to whole
seconds so a token never outlives its capability. All terms are bound with
`code_with_params`, never string formatting, so a path or name cannot inject
Datalog.

## Issue, attenuate, verify

```mermaid
flowchart LR
    I[Issuer root KeyPair] -->|issue| T0[authority block]
    T0 -->|attenuate| T1[+ block 1]
    T1 -->|attenuate| T2[+ block 2]
    T2 --> V[Verifier root PublicKey]
    V -->|verify_full| P["Verified {principal, capability}"]
```

* [`Issuer`](../../crates/rill-policy/src/tokens/issuer.rs) holds the root
  `KeyPair`. `issue(principal, capability)` returns a base64 token.
* `attenuate(token, root, narrower)` parses the parent, enforces
  `check_narrower_than` against its effective capability, and appends a
  block. A client that skips this check still cannot escape the parent,
  because Biscuit evaluates every block's checks.
* `inspect(token, root)` returns the effective capability without
  authorizing anything.
* `effective_capability` walks the block contexts in order, checks each one
  narrows the previous one, and returns the last. A block without context is
  `Error::Token`; a widening block is `Error::Widening`.

[`Verifier::verify_full(token, request)`](../../crates/rill-policy/src/tokens/verifier.rs):

1. Parse with the root key; a bad signature chain is `Error::Token`.
2. Compute `effective_capability`.
3. Expired at `request.now`: `Error::TokenExpired`.
4. Add `request_repo`, `request_path`, `request_verb`, `time` and optionally
   `request_workspace` facts, then `allow if true`. A failed check is
   `Error::TokenRejected`.
5. Read `principal` with `authorizer.query`, which sees only the authority
   block, so a `principal` fact in an attenuation block is ignored. The
   rebuilt `Principal` has `identity: None`.

`TokenRequest.now` is supplied by the caller, so verification is deterministic.

## Server use

[`BiscuitAuthenticator`](identity-and-auth.md) turns a valid token into an
`AuthContext` with `capability: Some(cap)`. Per-request enforcement is
`AuthContext::capability_allows` inside `Permit::check`. It is evaluated in
Rust rather than through Datalog, and before the relation check, even in dev
mode. The effective right is the capability AND the stored relations.

`AdminService.IssueToken`
([admin.rs](../../crates/rill-server/src/services/admin.rs)) mints tokens:

* Each `pb::Capability.scope` is `repo:<name>[/<dir>][/*]`. All scopes must
  name one repository.
* Actions accept `read`/`repo.read`, `write`/`repo.write`,
  `land`/`repo.land`/`lander.land`, `review`/`repo.review`/`review.vote`,
  `admin`/`repo.admin`.
* Expiry defaults to now plus 24 hours (`DEFAULT_TOKEN_TTL_MS`). A past value
  is `INVALID_ARGUMENT`.
* A caller holding a capability must pass `check_narrower_than`, which is the
  only check for that caller; widening is `INVALID_ARGUMENT`. A caller without
  a capability must pass `permit().admin()`.
* The subject is `for_principal`, or the caller.

`WhoAmI` reports the caller's capability as one `pb::Capability` per
verb and path, with action `repo.<verb>` and scope `repo:<name>/<path>/*`.

## Limitations

* The server never enforces `Capability::workspace` per request.
* There is no revocation. A token is valid until `expires_at`. Rotating the
  root key invalidates every token signed by it.
* `IssueToken` does not restrict `for_principal`. Any admin, or any token
  holder delegating a narrower scope, can mint a token for any principal name.
* Tokens carry no `identity` or teams.
* Clients do not call `attenuate`; delegation goes through `IssueToken`.

## Code

* [`tokens/capability.rs`](../../crates/rill-policy/src/tokens/capability.rs): `Capability`, `Verb`, `check_narrower_than`.
* [`tokens/datalog.rs`](../../crates/rill-policy/src/tokens/datalog.rs): `capability_block`, `authority_block`, `effective_capability`.
* [`tokens/issuer.rs`](../../crates/rill-policy/src/tokens/issuer.rs): `Issuer`, `attenuate`, `inspect`.
* [`tokens/verifier.rs`](../../crates/rill-policy/src/tokens/verifier.rs): `TokenRequest`, `Verified`, `Verifier`.
* [`tokens/tests.rs`](../../crates/rill-policy/src/tokens/tests.rs): issue, attenuate and reject cases.
* [`services/admin.rs`](../../crates/rill-server/src/services/admin.rs): `IssueToken`, `WhoAmI`, `parse_scope`, `parse_action`.

---
← Previous: [Identity and authentication](identity-and-auth.md) · [Index](../index.md) · Next: [Permissions](permissions.md) →

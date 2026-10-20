# Authentication and identity

Rill authenticates every gRPC and HTTP request from one `Authorization:
Bearer` header, resolved by a `CompositeAuthenticator` that consults a
static token table and then one or more Biscuit root keys. Identity is a
`Principal { id, kind, name, identity }` whose `id` is derived
deterministically from `(kind, name)`, so whoever controls a name controls
the identity. The invariant that matters most: a `Principal` attached to a
request must have been established by a credential the server verified,
and every record that names a principal as its actor must take that name
from the verified context, not from the request body. The second half of
that invariant does not hold today.

## Current state

### Credentials the server accepts

| Credential | Source of truth | Principal | Teams | Capability | Expiry | Revocation |
| --- | --- | --- | --- | --- | --- | --- |
| Static token ([config.rs](../../crates/rill-server/src/config.rs) `StaticToken`, `RILL_STATIC_TOKENS`) | TOML/env at startup | `Principal::{human,agent,service}(name)` | from config | none (`AuthContext::capability = None`) | never | edit config, restart |
| Biscuit token signed by the configured root ([authenticator.rs](../../crates/rill-server/src/auth/authenticator.rs)) | `root_public_key_hex` | authority block `principal(id, kind, name)` fact | none (`BiscuitAuthenticator::new` uses `NoTeams`) | from block contexts, `effective_capability` | `expires_at` in the capability | none |
| Biscuit token signed by this process's issuer | `Issuer` from `root_private_key_hex`, else `Issuer::generate()` per process | same | none | same | same | restart (when the key is ephemeral) |
| No header in `dev_mode` | config | `Principal::human("dev")` | none | none | | |

[identity.rs](../../crates/rill-policy/src/identity.rs) defines the
`Authenticator` trait and the two implementations;
[auth/layer.rs](../../crates/rill-server/src/auth/layer.rs) and
[auth/http.rs](../../crates/rill-server/src/auth/http.rs) run it for
tonic and axum respectively and attach `AuthContext` to request
extensions. `AuthHeaders::principal_hint` exists and is honoured by no
authenticator.

### Token issuance

`IssueToken` ([admin.rs](../../crates/rill-server/src/services/admin.rs)):

* Parses `capabilities[]` into one `Capability` (one repo, verbs, paths,
  expiry; default TTL 24 h, no maximum).
* If the caller holds a capability, the new one must be
  `check_narrower_than` it; otherwise the caller must pass
  `permit().admin()` (repo owner).
* `for_principal` is taken verbatim from the request. Nothing ties it to
  the caller: an owner can mint a token naming any human, and a capability
  holder can mint a narrower token for any principal. The log line records
  `by` and `for_principal`; nothing else does.
* `Issuer::issue` ([issuer.rs](../../crates/rill-policy/src/tokens/issuer.rs))
  signs an authority block with the principal fact and the capability
  checks; `attenuate` lets a holder append a narrower block offline.
* No `jti`/revocation id is added to blocks; the verifier never consults a
  revocation list. Biscuit supports `revocation_ids` natively; unused.

### Verification

`BiscuitAuthenticator::authenticate` runs `Verifier::verify_full` with a
probe `TokenRequest` built from the capability's own first path and verb,
so authentication proves signature chain, block consistency and expiry
only. Per-request enforcement is `AuthContext::capability_allows(repo,
path, verb)` in [permit.rs](../../crates/rill-server/src/auth/permit.rs),
which checks `repo`, `verbs` and `paths` and ignores
`Capability::workspace`. The server never calls `Verifier::verify` with a
real `TokenRequest`, so the Datalog `check if request_workspace(...)` in a
workspace-pinned token is never evaluated: a token "pinned" to one
workspace works on every workspace in the repository.

### Identity in records

* `Principal::human("alice")` ([principal.rs](../../crates/rill-types/src/principal.rs))
  derives `id` from the name; two deployments, or a static token and a
  Biscuit token, that spell the same name are the same principal. The
  `identity` field (email, SPIFFE id) is informational and never set by
  the server.
* `CreateChangeVersion` ([write.rs](../../crates/rill-server/src/services/repo/write.rs))
  stores `cv.author` from the request body unchanged. `ApplyEdits`,
  `Merge`, `NewChange` use `opened.principal()`. So an agent with write
  access can author a change as any human.
* `Provenance.sponsor` ([change.rs](../../crates/rill-types/src/objects/change.rs))
  is client-supplied and never compared with the caller or with any
  delegation record; it is not a Cedar attribute in the submit policy, so
  sponsorship is informational only.
* Signatures: `CreateChangeVersion` accepts `signatures[]` and
  `put_signatures` stores them under `sig/<repo>/<hash>` without
  verification. `rill_policy::signing` (`Signer`, `verify_signature`,
  [signing.rs](../../crates/rill-policy/src/signing.rs)) has no caller
  outside its own crate: the CLI, SDK, lander and reviewer never sign.
  `GetChangeVersion` returns stored signatures as if they meant something.
* Lander reverts run as `Principal::service("lander")`
  ([revert.rs](../../crates/rill-lander/src/revert.rs)); that service
  principal has no credential and cannot be impersonated, but also has no
  relations and bypasses permission checks because it never goes through
  a handler.

### Audit

`tracing::info!` lines exist for repository creation, token issuance,
policy reload, landings and reverts. There is no audit log as data: no
table, no `(who, what, when, from where)` record, no request id, no
correlation between a gRPC call and the store writes it caused. Review
state carries actors on votes and comments; the operation log carries
`actor` on workspace operations; `StoredPolicy` carries `updated_by`;
relation tuples carry nothing (no grantor, no timestamp).

### Tests

[authenticator.rs](../../crates/rill-server/src/auth/authenticator.rs)
tests static plus issued tokens; [tokens/tests.rs](../../crates/rill-policy/src/tokens/tests.rs)
covers narrowing, expiry and attenuation; `grpc_smoke.rs`
`static_auth_denies_unrelated_principal` covers one deny. Nothing tests
`for_principal` misuse, workspace pinning, team resolution for Biscuit
holders, key rotation, or the HTTP path with an expired token.

## Why it blocks production

1. **No SSO.** Every human needs a hand-maintained static token in a TOML
   file (or env var) that is a long-lived secret with no expiry; off-
   boarding is a config edit and a restart of every replica. Any
   organisation with an IdP will refuse this.
2. **Authorship spoofing.** An agent (or any writer) submits a
   `ChangeVersion` with `author = alice`; it lands with alice's name in
   `log`, `blame` and every review. Principle 6 (provenance is
   first-class and signed) is not enforced anywhere.
3. **Token minting for others.** A repository owner issues a token for
   `human:ceo`; a capability holder issues a narrower one for any name.
   Combined with name-derived ids, this is impersonation with a valid
   signature.
4. **No revocation.** A leaked 24-hour agent token stays valid for 24
   hours; a leaked one-year token (no maximum TTL) stays valid for a year.
   The only remedy is rotating the root key, which invalidates every token
   for every principal.
5. **Ephemeral issuer by default.** With `AuthConfig::Static` the issuer
   key is generated at startup; tokens handed to agents stop working on
   restart, and two replicas issue tokens the other rejects.
6. **Workspace pinning is a no-op.** The SDK and docs can hand an agent a
   token "for workspace W"; the agent can snapshot and commit to any
   workspace it can name.
7. **Teams invisible to token holders.** A `team:core` relation grants
   nothing to a Biscuit-authenticated member of `core`; operators will
   discover that ReBAC behaves differently per credential type.
8. **No audit trail.** A security incident ("who granted `owner` to the
   bot?") cannot be answered from the store.

## Requirements

* R1. OIDC: the server accepts an OIDC ID token or access token (JWT)
  from a configured issuer (`issuer_url`, `audience`, JWKS fetched and
  cached with rotation), maps `sub`/`email`/configured claim to a
  `Principal` with `identity = <issuer>|<sub>`, and maps a groups claim to
  `teams`. A human never holds a static token in production. The CLI
  performs the device-code or PKCE flow and caches the token.
* R2. Every actor field written by the server comes from `AuthContext`:
  `CreateChangeVersion` rejects or overwrites `author != caller` unless
  the caller holds an explicit `impersonate` capability (for importers).
* R3. `IssueToken.for_principal` is constrained: a human may issue for
  itself or for an agent principal whose `sponsor` is recorded as that
  human in a new `agent/<repo>/<agent-id>` record; a service may issue
  only for agents it registered; a repo owner may issue for any principal
  only with `Verb::Admin` and the event is audited.
* R4. Tokens carry a revocation id; the server keeps a revocation list in
  the `MetaStore` (`revoked/<id>` with expiry) that `authenticate` checks;
  `RevokeToken` RPC exists; a maximum TTL is configurable (default 24 h
  for agents, 1 h for delegated tokens).
* R5. The issuing key is required for non-dev configurations (startup
  fails without `root_private_key_hex` or a key file), and key rotation is
  supported: a list of trusted public keys with `not_after`, and the
  issuer signs with the newest.
* R6. `Capability::workspace` is enforced: every workspace RPC passes the
  workspace id into the permission check, and non-workspace RPCs are
  denied for a pinned token except reads.
* R7. Teams are resolved for every credential type through one
  `TeamResolver` backed by the IdP claims (OIDC) or a `teams/<team>/<id>`
  table (static and Biscuit).
* R8. Signatures are verified on store: `put_signatures` only stores a
  signature whose `signer` key is registered for the caller's principal
  (`keys/<principal-id>/<fingerprint>`), and the server signs every landed
  version with a server key so provenance can be checked offline.
* R9. An append-only audit table `audit/<be64 seq>` records every
  mutating RPC, every authentication failure class, every token issuance
  and revocation and every relation change, with `(at, principal,
  identity, method, repo, target, outcome, request_id, peer)`. It is
  exportable and GC-exempt.

## Design direction

### Recommended: OIDC for humans, server-registered agents with sponsored
Biscuit tokens, one `AuthContext` as the only source of actors

```mermaid
sequenceDiagram
    participant H as Human (CLI)
    participant IdP as OIDC provider
    participant S as rill-server
    participant M as MetaStore
    participant A as Agent
    H->>IdP: device-code / PKCE login
    IdP-->>H: ID token (sub, email, groups)
    H->>S: RPC, Bearer <JWT>
    S->>IdP: JWKS (cached)
    S->>S: Principal{human, identity=iss|sub}, teams=groups
    H->>S: RegisterAgent(name) → agent/<id> {sponsor: H}
    H->>S: IssueToken(for=agent, caps, ttl≤max)
    S->>M: audit append; token revocation id recorded
    S-->>H: Biscuit (authority: principal, sponsor, rev-id)
    H->>A: hands token
    A->>S: RPC, Bearer <Biscuit>
    S->>M: get revoked/<rev-id>
    S->>S: AuthContext{agent, sponsor=H, capability}
    A->>S: CreateChangeVersion(author=agent, provenance.sponsor=H)
    S->>S: author := ctx.principal; sponsor := ctx.sponsor
```

Components:

1. **`OidcAuthenticator`** in `rill-policy` (no tonic dependency): JWKS
   fetch through a trait the server implements with `reqwest`; validates
   `iss`, `aud`, `exp`, signature; claim mapping in config
   (`auth.oidc.{issuer, audience, principal_claim, teams_claim}`).
   `CompositeAuthenticator` order: static (if configured, dev only),
   OIDC, Biscuit roots.
2. **Agent registry.** `agent/<repo>/<agent-id>` record `{name, sponsor,
   created_by, created_at, disabled}`; `RegisterAgent`, `DisableAgent`
   RPCs. `IssueToken` for an agent requires the record and sets a
   `sponsor(id)` fact in the authority block; `AuthContext` gains
   `sponsor: Option<Principal>`; `commit_version` sets
   `provenance.sponsor` from it and rejects a mismatch.
3. **Actor binding.** `commit_version` enforces `cv.author ==
   ctx.principal` unless `ctx.capability` includes a new `Verb::Impersonate`
   limited to the git importer's service principal. Same rule for
   `signatures[].signer`.
4. **Revocation.** Each issued token gets a `revocation_id` (ULID) as a
   Biscuit fact and in the block context; `revoked/<id> → expires_at`;
   `authenticate` does one `get` (cached for a few seconds). Rows expire
   with the token so the table stays bounded.
5. **Key management.** `auth.biscuit.keys = [{public, private?,
   not_after}]`; verification tries every unexpired public key; signing
   uses the one with a private half and the latest `not_after`. Rotation
   is adding a key, waiting the maximum TTL, removing the old one.
6. **Workspace pinning.** `Permit::check` takes an optional
   `WorkspaceId`; `capability_allows` compares it; workspace handlers pass
   `ws.id`.
7. **Audit.** A `tower` layer around the gRPC router and an axum middleware
   emit one `AuditEvent` per request after the handler returns, written by
   a background task with an `AppendLog` into `audit/<repo>` (or
   `audit/_global` for admin calls without a repository); also emitted as
   a `tracing` event for log shipping. Request id from an incoming
   `x-request-id` or generated.

### Alternatives considered

* **Keep static tokens, add a token file reload.** Cheaper, but does
  nothing for identity assurance or off-boarding; acceptable only as the
  dev path.
* **mTLS / SPIFFE for everything.** Strong for services and agents in a
  mesh; poor for human CLIs. Worth supporting as a third authenticator
  later (the `identity` field already anticipates SPIFFE ids).
* **JWT for agents instead of Biscuit.** Loses offline attenuation, which
  is the feature that lets a sponsor hand sub-agents narrower tokens
  without a server round-trip. Keep Biscuit.
* **Server-side sessions instead of bearer tokens.** Adds state per
  connection and a cookie model that does not fit gRPC well. Rejected.

### Trade-offs

OIDC adds a network dependency on the IdP for JWKS (cached; outages only
block new keys) and a clock dependency. Binding `author` to the caller
breaks the current CLI flow where a client constructs the `ChangeVersion`
locally with an author it chose; the SDK must fill `author` from `WhoAmI`
or leave it for the server to set.

## Work breakdown

1. **S** Bind `author`, `provenance.sponsor` and `signatures[].signer` to
   `AuthContext` in `commit_version`; reject mismatches; test.
2. **S** Enforce `Capability::workspace` through `Permit`; test with a
   pinned token on two workspaces.
3. **S** Require an issuing key outside dev mode; add key list with
   `not_after`; rotation test.
4. **M** Revocation ids in tokens, `revoked/` table, `RevokeToken` RPC,
   max TTL config; test that a revoked token fails on gRPC and HTTP.
5. **M** `TeamResolver` backed by a `teams/` table plus `SetTeams` admin
   RPC; wire it into `BiscuitAuthenticator`; test team tuple with a
   Biscuit holder.
6. **L** `OidcAuthenticator` with JWKS cache and claim mapping; CLI device-
   code/PKCE login and token cache; integration test against a mock IdP
   (e.g. a local JWKS server in the test).
7. **M** Agent registry records and RPCs; `for_principal` constraints in
   `IssueToken`; `sponsor` fact and `AuthContext::sponsor`.
8. **M** Audit layer, `audit/` table, `ListAudit` admin RPC, export hook
   for [03](03-backup-restore-and-migration.md).
9. **M** Signature verification on store, server-side landing signatures,
   key registration RPC; make the CLI sign when a key is configured.

## Risks and open questions

* Principal ids derived from names: with OIDC the stable id should be
  `iss|sub`, which breaks `PrincipalId::derived("human", name)` equality
  for existing records. A migration that maps old ids to new ones, or a
  rule that `name` is the IdP `preferred_username` and id derivation
  includes the issuer, needs a decision before OIDC ships.
* `svc:lander` acts without a credential; if audit records every actor,
  internal service principals need a documented "system" origin.
* Biscuit `time()` facts are whole seconds; expiry rounding down is
  already handled. Clock skew between replicas and the IdP is the usual
  5-minute leeway question.
* `dev_mode` grants `human:dev` everything; it must refuse to start on a
  non-loopback listen address (see [06](06-operations-and-observability.md)).
* Whether the git importer should carry original git authors as `author`
  (needs `Impersonate`) or as `provenance.extra` with the service as
  author. The former preserves blame; the latter preserves the invariant.

## References

Source:

* [crates/rill-server/src/auth/authenticator.rs](../../crates/rill-server/src/auth/authenticator.rs)
* [crates/rill-server/src/auth/context.rs](../../crates/rill-server/src/auth/context.rs)
* [crates/rill-server/src/auth/layer.rs](../../crates/rill-server/src/auth/layer.rs)
* [crates/rill-server/src/auth/http.rs](../../crates/rill-server/src/auth/http.rs)
* [crates/rill-server/src/auth/permit.rs](../../crates/rill-server/src/auth/permit.rs)
* [crates/rill-server/src/config.rs](../../crates/rill-server/src/config.rs)
* [crates/rill-server/src/services/admin.rs](../../crates/rill-server/src/services/admin.rs)
* [crates/rill-server/src/services/repo/write.rs](../../crates/rill-server/src/services/repo/write.rs)
* [crates/rill-policy/src/identity.rs](../../crates/rill-policy/src/identity.rs)
* [crates/rill-policy/src/tokens/capability.rs](../../crates/rill-policy/src/tokens/capability.rs)
* [crates/rill-policy/src/tokens/issuer.rs](../../crates/rill-policy/src/tokens/issuer.rs)
* [crates/rill-policy/src/tokens/verifier.rs](../../crates/rill-policy/src/tokens/verifier.rs)
* [crates/rill-policy/src/tokens/datalog.rs](../../crates/rill-policy/src/tokens/datalog.rs)
* [crates/rill-policy/src/signing.rs](../../crates/rill-policy/src/signing.rs)
* [crates/rill-types/src/principal.rs](../../crates/rill-types/src/principal.rs)
* [crates/rill-lander/src/revert.rs](../../crates/rill-lander/src/revert.rs)

Prior art:

* Biscuit specification: revocation ids, third-party blocks, `time`
  facts; the `biscuit-auth` crate's `RevocationIds`.
* OpenID Connect Core, Device Authorization Grant (RFC 8628), PKCE (RFC
  7636); GitHub CLI and `gcloud auth login` as reference CLI flows.
* SPIFFE/SPIRE for workload identity; the `identity` field format.
* Gerrit's account/external-id model and its `--impersonate` style
  "forge author/committer" capabilities, which is the shape `Impersonate`
  should take.
* Sigstore `gitsign` and git's `commit.gpgsign` for signing landed
  versions; Gerrit and GitHub "verified" badges as the UX target.
* Macaroons (Birgisson et al. 2014) for the caveat/attenuation model that
  Biscuit generalises.
* Google Zanzibar (Pang et al. 2019) for the audit of ACL changes
  alongside the tuple store.

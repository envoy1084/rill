# Security review packet

What a reviewer of Rill's security design needs in one place: the scope,
where each control lives and how to check it, what is known to be missing,
and the findings of the last review. It complements the
[threat model](threat-model.md), which lists threats and their state.

## Scope

The server (`rill-api` and the crates below it), the cache tier
(`rill-cache`), and the SDK and CLI as they talk to the server. Out of
scope: client checkouts on developer machines (phase 20), sandboxed
execution of verification steps (phase 15), and the release pipeline
(phase 30).

## Trust boundaries

| Boundary | Crossed by | Checked by |
| --- | --- | --- |
| Network → server | gRPC, object HTTP, SCIM, sign-in | TLS (optionally mutual); every route authenticates except health, sign-in and `/public-keys` |
| Caller → repository | every repository RPC | `Permit`: token capability ∩ relations (ReBAC) ∩ agent scope; the [permission matrix](permissions.md) test covers every RPC |
| Caller → object bytes | `GetBlob`, `GetTree`, `/o`, batch | signed read grants naming exact hashes, checked by the server and the cache |
| Writer → trunk | landing, `AppendTrunk` | write sets authorized at commit and at land; submit policy; quarantine |
| Org → org | everything | per-org metadata keys and object namespaces; per-org data keys; per-org audit chains |
| Server → storage | object store, metadata store | objects sealed per org; storage credentials and the key service are trusted |
| Administrator → history | redaction, policy, keys | two-person redaction; every administrative change in the audit chain |

## Controls and where to verify them

| Control | Code | Evidence |
| --- | --- | --- |
| Authentication and tokens | [identity and auth](identity-and-auth.md), [token keys](token-keys.md) | `rill-api/tests/grpc/auth.rs`, `signin.rs`, `killswitch.rs` |
| Authorization | [permissions](permissions.md) | `tests/grpc/access/` (matrix, holes, grants, writes) |
| Audit log | [audit log](audit.md) | `tests/grpc/audit.rs` (tamper detection), store conformance `audit.rs`; `rill admin audit verify` |
| Secret scanning, quarantine | [data protection](data-protection.md#secret-scanning) | `tests/grpc/protection.rs` (block on every write path, quarantine at land and `AppendTrunk`) |
| Redaction | [data protection](data-protection.md#redaction) | `tests/grpc/protection.rs` (two admins; gRPC, `/o`, batch, re-upload, search), `rill-cli/tests/cache.rs` (cache drops and refuses) |
| Encryption at rest | [data protection](data-protection.md#encryption-at-rest) | `rill-store/src/seal/org.rs` tests, `tests/grpc/encryption.rs` (nothing on disk in the clear, cross-org copy refused, rotation, a server without the key service reads nothing) |
| Input limits | [API surface](../07-runtime/api-surface.md#input-limits) | `tests/grpc/limits.rs`, `rill-types/src/limits.rs` tests |
| Untrusted parsers | [testing](../09-engineering/testing.md#fuzzing) | twelve `cargo-fuzz` targets |

## Known gaps

| Gap | Effect | Planned |
| --- | --- | --- |
| Verification steps run unsandboxed on the server | a repository writer can run code on the server when verification is on | 15 |
| Clients follow symlinks when writing checkouts | malicious content could write outside a checkout on a client | 20 |
| Agent votes count toward agent quorums | a swarm could approve its own work | 23 |
| No re-encryption under rotated data keys | old objects stay under their original key | 28 |
| No reproducible or signed releases | a backdoored binary would go unnoticed | 30 |
| Fuzzing is not continuous | parsers are fuzzed when they change, not around the clock | — |
| Metadata other than secrets is stored in the clear | names, paths and review state rely on disk encryption | — |
| Cache disks hold plaintext; redactions reach a cache within one poll | an operator must encrypt cache volumes | — |

## Review findings

The review of the Phase 12 design and code (audit, secret scanning,
redaction, encryption at rest, input limits) found no criticals. Every
finding below was reproduced by a test that failed before its fix.

| Finding | Severity | State |
| --- | --- | --- |
| A subtree copy brought content onto trunk unscanned (and around quarantine) | high | fixed (`86fbfa5`) |
| Content at an allowed path was cached as clean, so the same bytes passed elsewhere | high | fixed (`86fbfa5`) |
| One commit with many findings could tie up a server (quadratic work on an async thread, unbounded audit detail) | high | fixed (`86fbfa5`) |
| `AppendTrunk` moved trunk to a quarantined version | high | fixed (`3e16153`) |
| Cached tree nodes were served after their redaction | medium | fixed (`7ff78e7`) |
| Two concurrent seals stored overlapping roots, so verification failed | medium | fixed (`82b7e5d`) |
| A child of a quarantined version could reach trunk | low | fixed (`86fbfa5`) |
| One NUL byte (or UTF-16) turned scanning off for a file | low | fixed (`86fbfa5`) |
| Change messages were not scanned | low | fixed (`86fbfa5`) |
| `GET /redactions` was public; a cache that could not reread it kept serving | low | fixed (`4885996`) |
| Inline files (≤ 256 bytes) cannot be redacted by their own hash | medium | by design: redact the trees holding them; block mode keeps credentials out |
| An org administrator who controls a second admin account can satisfy two-person redaction alone | low | by design: both accounts are audited; separate identity control is the operator's |

## Code

* This page has no code of its own; each row above links to its owner.

---
← Previous: [Threat model](threat-model.md) · [Index](../index.md) · Next: [Search](../06-services/search.md) →

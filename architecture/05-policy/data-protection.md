# Data protection

What keeps credentials out of history and what removes content that must
not be served: secret scanning on every write, quarantine, and redaction
approved by two administrators.

## Secret scanning

Every version is scanned as it is committed
([`services/repo/secrets.rs`](../../crates/rill-api/src/services/repo/secrets.rs)),
whatever the entry point: `ApplyEdits`, `CreateChangeVersion`, rebases and
merges, workspace snapshots and imports all store versions through the same
commit path. Only new content is read: inline bytes, and blobs and manifests
streamed part by part with a 4 KiB overlap so a secret split across two
chunks is still found. Content found clean is remembered per rules digest,
so a file is scanned once however many versions name it.

The detectors ([`rill-policy/src/secrets`](../../crates/rill-policy/src/secrets/))
are pure: AWS keys, GitHub, Slack, Stripe and Google keys, GCP service
account keys, private keys, JWTs, Rill refresh tokens and SCIM credentials,
plus a generic `key = "…"` assignment. Each has a minimum Shannon entropy, so
placeholders like `AKIAEXAMPLEEXAMPLE00` do not trip it.

A finding names the detector, the path, the line and a fingerprint (a keyed
hash prefix of the matched text). The secret itself is never stored, logged,
audited or returned.

### Rules

A repository's rules are `.rill/secrets.toml` on trunk, read at the current
trunk head; with no file, the defaults apply. Unreadable rules fall back to
the defaults rather than disabling scanning.

```toml
mode = "block"                    # or "warn", "quarantine"
allow_paths = ["tests/fixtures/**"]
allow_fingerprints = ["3f2a9c…"]  # a known fake, from its finding
[[pattern]]
name = "acme-deploy-key"
regex = 'acme_dk_[0-9a-f]{40}'
```

| Mode | The version | Audit |
| --- | --- | --- |
| `block` (default) | refused with `FailedPrecondition` naming the detector and path | `secret.block`, written even though the version is not |
| `warn` | stored | `secret.warn`, in the same transaction |
| `quarantine` | stored, but the lander refuses it until an owner releases it (`ClearQuarantine`, audited `quarantine.clear`) | `secret.quarantine`, in the same transaction |

## Redaction

Content-addressed history cannot be rewritten, so removing an object means
refusing to serve it. A redaction names an org, an object kind and a hash.
One administrator requests it with a reason, and a second one approves it.
Approving one's own request is refused. Approval writes `RedactionApplied`
to the identity log, which every server applies to its identity view
([`rill-store/src/identity/redactions.rs`](../../crates/rill-store/src/identity/redactions.rs)).

Each repository's object store is wrapped in a `RedactingObjectStore`
([`rill-store/src/redact.rs`](../../crates/rill-store/src/redact.rs)) that
consults the redacted set. Every read and write of a redacted hash fails
with `Error::Redacted`:

* gRPC returns `FailedPrecondition`, and HTTP `/o` returns 451;
* re-uploading the same bytes is refused, so the content cannot come back
  under its hash;
* search and grep treat it as empty rather than failing;
* `GET /redactions` publishes the redacted hashes (hashes only). Cache tiers
  poll it, then drop their copies and refuse those hashes.

The bytes stay in the object store, unreachable through Rill. Physically
deleting them is the backing store's lifecycle job.

Requests and approvals are audited as `redaction.request` and
`redaction.apply`.

## Limitations

* Scanning sees only content that reaches the server. A secret in an
  unpushed local commit is the client's concern.
* Rules come from trunk, so a change cannot loosen the rules it is checked
  under. Changing them takes an owner-approved land.
* A cache tier serves a redacted object until its next poll
  (`--redaction-poll`, 10 s by default).
* Redaction is by exact hash. Other copies (an edited file, another chunk
  boundary) need their own redactions.

## Code

* [`rill-policy/src/secrets/`](../../crates/rill-policy/src/secrets/): detectors, rules, the scanner.
* [`rill-api/src/services/repo/secrets.rs`](../../crates/rill-api/src/services/repo/secrets.rs), [`commit.rs`](../../crates/rill-api/src/services/repo/commit.rs): scanning at commit.
* [`rill-api/src/services/protection.rs`](../../crates/rill-api/src/services/protection.rs): `ProtectionService`.
* [`rill-store/src/repo/quarantine.rs`](../../crates/rill-store/src/repo/quarantine.rs), [`redact.rs`](../../crates/rill-store/src/redact.rs).
* [`rill-cache/src/redactions.rs`](../../crates/rill-cache/src/redactions.rs).
* Tests: `rill-api/tests/grpc/protection.rs`, `rill-cli/tests/cache.rs`.

---
← Previous: [Audit log](audit.md) · [Index](../index.md) · Next: [Threat model](threat-model.md) →

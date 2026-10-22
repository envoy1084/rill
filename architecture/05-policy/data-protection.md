# Data protection

What keeps credentials out of history, what removes content that must not
be served, and what keeps stored bytes unreadable without the key service:
secret scanning on every write, quarantine, redaction approved by two
administrators, and per-org encryption at rest.

## Secret scanning

Every version is scanned as it is committed
([`services/repo/secrets.rs`](../../crates/server/runtime/src/secrets.rs)),
whatever the entry point: `ApplyEdits`, `CreateChangeVersion`, rebases and
merges, workspace changes (`NewChange`) and imports all store versions
through the same commit path, which checks the author's rights (including
read on every copy source) before scanning. What a version adds is read:
its message, paths and extension fields, the files it sets (inline bytes,
and blobs and manifests streamed part by part with a 4 KiB overlap so a
secret split across two chunks is still found), and every file of a
directory it copies, at its destination path; copied files count toward
the 100,000-file limit before any is read. Binary-looking content is
scanned as its printable runs and as its even and odd bytes, which covers
UTF-16 of either byte order. Scanning runs on the blocking pool and stops
at 20 findings per file and 100 per version; a version that reaches a
limit is refused whatever the mode, since nobody could review what was not
listed. An audit record names at most 20 findings. Content found clean is remembered per rules
digest, so a file is scanned once however many versions name it; content
at an allowed path is never remembered as clean.

The detectors are [gitleaks](https://github.com/gitleaks/gitleaks)' rule
set (about 220 rules, MIT), vendored unmodified in
[`rill-policy/rules/gitleaks.toml`](../../crates/domain/policy/rules/) and
applied as gitleaks applies them: a case-insensitive keyword prefilter
chooses the rules to run, each rule's secret group and minimum entropy
decide a match, rule and global allowlists (regexes, stopwords, paths)
drop known non-secrets, and paths gitleaks never scans (lockfiles,
vendored dependencies, images, binaries) are skipped. Two Rill detectors
cover the credentials Rill itself issues (refresh tokens `ss_…`, SCIM
credentials `scim_…`). Updating the rules is replacing the vendored file.
The catalog compiles once per process, warmed at server start.

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
| `quarantine` | stored, but neither the lander nor `AppendTrunk` moves trunk to it, or to any version descending from it, until an owner releases it (`ClearQuarantine`, audited `quarantine.clear`) | `secret.quarantine`, in the same transaction |

## Redaction

Content-addressed history cannot be rewritten, so removing an object means
refusing to serve it. A redaction names an org, an object kind and a hash.
One administrator requests it with a reason, and a second one approves it.
Approving one's own request is refused. Approval writes `RedactionApplied`
to the identity log, which every server applies to its identity view
([`rill-store/src/identity/redactions.rs`](../../crates/foundation/store/src/identity/redactions.rs)).

Each repository's object store is wrapped in a `RedactingObjectStore`
([`rill-store/src/redact.rs`](../../crates/foundation/store/src/redact.rs)) that
consults the redacted set. Every read and write of a redacted hash fails
with `Error::Redacted`:

* gRPC returns `FailedPrecondition`, and HTTP `/o` returns 451;
* re-uploading the same bytes is refused, so the content cannot come back
  under its hash;
* search and grep treat it as empty rather than failing;
* caches of decoded objects (tree nodes) ask the store before serving a
  hit, so a redacted tree is refused even where it was cached;
* `GET /redactions` gives the redacted hashes to service principals (cache
  tiers) and instance administrators. Caches poll it, drop their copies and
  refuse those hashes, and serve nothing while their copy of the list is
  more than a minute stale.

The bytes stay in the object store, unreachable through Rill. Physically
deleting them is the backing store's lifecycle job.

Requests and approvals are audited as `redaction.request` and
`redaction.apply`.

## Encryption at rest

Objects on local disk and in S3 are sealed with AES-256-GCM under their
org's data keys ([`rill-store/src/seal/org.rs`](../../crates/foundation/store/src/seal/org.rs)).
Sealing is on by default outside dev mode (`[encryption] enabled`,
`RILL_ENCRYPTION`); in-memory storage is never sealed.

* **Data keys.** Each org has its own 32-byte random keys, wrapped by the
  instance's [key service](token-keys.md#key-services) and stored as
  `DataKeyRecord`s in identity metadata (`(dk, org, id)`). An org gets its
  first key, active at once, when its first object is sealed. Servers
  unwrap each key once and keep it in memory.
* **Frames.** A sealed object is `RXE1 ‖ key id ‖ nonce ‖ ciphertext ‖ tag`
  around the compressed frame. Every object is encrypted under its own key,
  derived (keyed BLAKE3) from the data key, kind and hash, with a random
  nonce, so no nonce budget applies. The associated data binds the org,
  kind, hash and key id, so a frame copied to another org or hash fails
  authentication. Plaintext frames are refused when sealing is on.
* **Rotation.** `rill admin protect rotate-key --org O` adds a key that
  seals new objects ten minutes later; servers re-read an org's keys every
  minute, so all of them hold it first. Old keys stay to open what they
  sealed. `--rewrap` wraps every data key again under the key service's
  current key. Re-encrypting old objects under new keys comes later.
* **Metadata.** Secrets in metadata are wrapped (token keys, data keys,
  identity provider client secrets), hashed (refresh secrets, SCIM
  credentials) or single-use and short-lived (sign-in PKCE verifiers, ten
  minutes). The rest of metadata (names, paths, relations, review state)
  is not encrypted by Rill; FoundationDB and redb files rely on disk
  encryption.

## Limitations

* Scanning sees only content that reaches the server. A secret in an
  unpushed local commit is the client's concern.
* Rules come from trunk, so a change cannot loosen the rules it is checked
  under. Changing them takes an owner-approved land.
* A cache tier serves a redacted object until its next poll
  (`--redaction-poll`, 10 s by default), and needs a service token to poll.
* Cache tiers keep plaintext objects on their own disks; encrypt those
  volumes.
* Losing the key service (or its key) loses every object. Back it up as
  carefully as the data.
* Files of at most 256 bytes are stored inline in their tree, with no hash
  of their own; redacting them means redacting the trees that hold them.
  Scanning in block mode keeps credentials from being stored at all.
* Redaction is by exact hash. Other copies (an edited file, another chunk
  boundary) need their own redactions.

## Code

* [`rill-policy/src/secrets/`](../../crates/domain/policy/src/secrets/): the rule catalog, repository rules, the scanner; [`rill-policy/rules/`](../../crates/domain/policy/rules/): the vendored gitleaks rules.
* [`rill-api/src/services/repo/secrets.rs`](../../crates/server/runtime/src/secrets.rs), [`commit.rs`](../../crates/server/api/src/services/repo/commit.rs): scanning at commit.
* [`rill-api/src/services/protection.rs`](../../crates/server/api/src/services/protection.rs): `ProtectionService`.
* [`rill-store/src/repo/quarantine.rs`](../../crates/foundation/store/src/repo/quarantine.rs), [`redact.rs`](../../crates/foundation/store/src/redact.rs).
* [`rill-cache/src/redactions.rs`](../../crates/server/cache/src/redactions.rs).
* [`rill-store/src/seal/`](../../crates/foundation/store/src/seal/): the `Sealer` hook and `OrgSealer`; [`rill-identity/src/keys/data.rs`](../../crates/domain/identity/src/keys/data.rs): `DataKeyManager`.
* Tests: `rill-api/tests/grpc/protection.rs`, `rill-api/tests/grpc/encryption.rs`, `rill-cli/tests/cache.rs`.

---
← Previous: [Audit log](audit.md) · [Index](../index.md) · Next: [Threat model](threat-model.md) →

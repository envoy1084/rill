# Signing

`signing.rs` produces and checks Ed25519 signatures over object hashes. A
`Signer` holds a keypair. `sign` returns a detached `Signature` that names the
signing principal, and `verify` checks it against the public key the signature
carries. The invariant: the signed message is `"rill:sig:" || hash`, so a
signature over a Rill object hash cannot be replayed as a signature over other
data signed with the same key.

## What is signed

A signature covers an `ObjectHash`, the BLAKE3 digest of a kind-prefixed
encoding. In practice the signed object is a change version. The version hash
covers its author, provenance, parents and file changes, so a signature over
it attests to the whole version, including
[`Provenance`](../../crates/rill-types/src/objects/change.rs).

```mermaid
flowchart LR
    CV[ChangeVersion] -->|encode + digest| H[ChangeVersionHash]
    H -->|"prefix rill:sig:"| M[message]
    K[Signer] -->|Ed25519 sign| S[64-byte signature]
    S --> R["Signature {signer, public_key, signature, signed_at}"]
    R -->|put_signatures| ST["sig/<repo>/<hash>"]
```

| `Signature` field | Content |
| --- | --- |
| `signer` | The `Principal` the key signed for. |
| `public_key` | 32-byte Ed25519 verifying key. |
| `signature` | 64-byte signature over `rill:sig:` + hash bytes. |
| `signed_at` | Informational. Not covered by the signature. |

## Keys and API

[`Signer`](../../crates/rill-policy/src/signing.rs):

* `generate()` uses `OsRng`. `from_seed(&[u8; 32])` is deterministic.
  `from_bytes` rejects any length other than 32 with `Error::InvalidKey`.
* `seed()` returns the secret, and `public_key()` returns 32 bytes. `Debug`
  prints only the public key.
* `sign(hash, principal)` is `sign_at(hash, principal, Timestamp::now())`.

`verify(sig, hash)` rebuilds the message and runs
`VerifyingKey::verify_strict`. A key or signature of the wrong length returns
`false` and never panics. A `true` result proves only that the embedded key
signed this hash. It does not prove that the key belongs to `sig.signer`; that
binding is the caller's job.

`KeyFile::encode` writes the seed as 64 hex characters plus a newline.
`decode` trims whitespace and hex-decodes. `encode_public` and `decode_public`
do the same for verifying keys. The crate defines this format but performs no
file I/O.

## Provenance

`Provenance` is part of the hashed version, so a version signature also fixes
`tool`, `model`, `session`, `prompt_ref`, `sponsor` (the human who delegated an
agent's work), `resolved_by` and the free-form `extra` map. The server sets
`resolved_by` from `Permit::resolved_by()` in `ApplyEdits`, and copies
`sponsor` forward on amends when the new version omits it. The risk classifier
bumps a change whose `resolved_by` is `agent`; see
[risk classification](risk-classification.md).

## Server path

`CreateChangeVersion` accepts `signatures` with the version. It converts each
one with `Signature::try_from`, commits the version, and then calls
`RepoStore::put_signatures(&hash, &signatures)` when the list is non-empty.
`GetChangeVersion` returns `get_signatures(&hash)` with the version. The SDK
exposes both. Signatures are stored as one record-encoded `Vec<Signature>`
under `sig/<repo>/<hash>`.

The server never calls `verify`, never checks `sig.signer` against the caller,
and never signs anything itself. `ApplyEdits` and the lander produce unsigned
versions.

## Limitations

* `Signer` and `verify` have no callers outside `rill-policy`. The CLI, SDK
  and server do not sign or verify.
* Signatures are written after the version commit. A failure between the two
  leaves an unsigned version.
* `put_signatures` replaces the whole list. A second `CreateChangeVersion` for
  the same hash with different signatures overwrites the first list.
* No key directory binds public keys to principals, so a stored signature
  proves possession of some key, not the identity in `signer`.
* `signed_at` is unsigned metadata.
* Signing keys are unrelated to the Biscuit root key used for
  [capability tokens](capability-tokens.md).

## Code

* [`signing.rs`](../../crates/rill-policy/src/signing.rs): `Signer`, `verify`, `KeyFile`.
* [`objects/change.rs`](../../crates/rill-types/src/objects/change.rs): `Signature`, `Provenance`.
* [`rill-store/src/repo/versions.rs`](../../crates/rill-store/src/repo/versions.rs): `put_signatures`, `add_signatures`, `get_signatures`.
* [`services/repo/write.rs`](../../crates/rill-server/src/services/repo/write.rs): `create_change_version_impl`, provenance handling.
* [`services/repo/history.rs`](../../crates/rill-server/src/services/repo/history.rs): signatures on read.
* [`rill-sdk/src/repo/write.rs`](../../crates/rill-sdk/src/repo/write.rs), [`rill-sdk/src/repo/history.rs`](../../crates/rill-sdk/src/repo/history.rs): client surface.

---
← Previous: [Submit policy](submit-policy.md) · [Index](../index.md) · Next: [Threat model](threat-model.md) →

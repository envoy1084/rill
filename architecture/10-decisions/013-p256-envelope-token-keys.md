# 013 · P-256 everywhere, with token keys wrapped by a key service

Status: accepted

## Context

Tokens are Biscuits and commit attestations are signatures, and both need
keys an operator can rotate and keep out of config files and disks.
Companies expect those keys to sit behind their KMS (AWS KMS, Google Cloud
KMS, Vault). Biscuit signs every block with an in-memory key and has no
external-signer hook, and Ed25519, its default, is not offered for signing
by every KMS. P-256 is offered by all of them and supported by Biscuit.

## Decision

* Every key Rill uses is P-256: token root keys, the ephemeral key of every
  Biscuit block, and attestation keys. No other curve is accepted.
* Token signing keys are envelope keys. A server generates the key, wraps
  its secret with the configured key service, and stores the wrapped secret
  and the public key in the metadata store under a numeric key id that
  tokens carry (Biscuit's root key id). Servers unwrap only the key they
  sign with and verify with public keys alone.
* Keys rotate on a schedule. A new key is announced through the identity
  log before it signs anything; the key it replaces verifies until every
  token it could have signed has expired, then retires.

## Consequences

* The key service is called when a server loads a key, not per token.
* Changing the key service mints a new key; stored keys it cannot open still
  verify until they retire.
* A metadata-store copy alone does not reveal a signing key.
* Token formats with Ed25519 keys are refused.

## Alternatives rejected

* **Signing tokens inside the KMS:** Biscuit cannot delegate its signature,
  and a KMS call per token would bound issuance by KMS latency and quotas.
* **Keys in configuration files:** no rotation, and secrets on disk.
* **Ed25519:** not available for signing in every KMS.

## Revisit when

Biscuit grows an external-signer interface, or a target KMS drops P-256.

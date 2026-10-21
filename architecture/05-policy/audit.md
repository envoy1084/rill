# Audit log

Every org has an audit log: who did what to which thing, when, and whether
it was allowed. A per-org hash chain makes any later edit or deletion
detectable, and signed seals pin the chain at points an auditor can check
offline. Instance-wide actions (signing keys, revocations, org creation)
go in the `_instance` stream.

## What is audited

Audit records are not written by each handler. Every state change already
writes, in its own transaction, either an identity log entry or a
repository event; the audit worker projects those
([`audit/project.rs`](../../crates/rill-api/src/audit/project.rs)). An
action is therefore audited exactly when it happens.

| Source | Actions |
| --- | --- |
| Identity log | `keys.change`, `token.revoke`, `session.revoke`, `session.open` (sign-in and workload exchange), `token.issue` (`IssueToken` and `mint-token`), `user.change`, `idp.change`, `agent.change`, `trust.change`, `team.change`, `org.settings`, `org.create`, `scim.credential.create`/`revoke` |
| Repository events | `repo.create`, `relation.add`/`remove`, `policy.set`, `ref.move`, `review.submit`, `review.vote`, `land.request`, `land.refuse`, `land` |
| Refusals | `repo.<verb>` with outcome `Denied`, from every `Permit` refusal, sampled to one per principal and repository a minute and written best effort ([`audit/denials.rs`](../../crates/rill-api/src/audit/denials.rs)) |

Stored versions, workspace operations, comments and intents are routine
work, not audited. The kill switch is audited through the changes it makes
(`agent.change`, `session.revoke`, `org.settings`).

An `AuditRecord` ([types](../../crates/rill-types/src/audit/entry.rs)) holds:

* the org;
* the time;
* the actor principal;
* a dotted action;
* a target (`repo:web`, `user:ann`, `token:tk_…`);
* the outcome (`Allowed`, or `Denied { reason }`);
* string details such as the subject of a relation, a vote's label and value, or a landing's sequence.

## The chain

Records are first queued in `(audq, org, stamp)`. The worker
([`audit/worker.rs`](../../crates/rill-api/src/audit/worker.rs)) then
places them ([`rill-store/src/audit/`](../../crates/rill-store/src/audit/mod.rs)):

* `AuditEntry { seq, prev, record }` at `(aud, org, seq)`, where `prev` is
  the hash of entry `seq - 1` (zeros for the first);
* an entry's hash is BLAKE3 (`derive_key("rill 2026 audit entry")`) over
  its postcard encoding;
* the head `(audh, org)` holds the last entry's number and hash. It moves
  by compare-and-set, so concurrent chainers never fork a chain or chain a
  record twice;
* the projection cursors `(audc, source)` (one for the identity log, one
  per repository) move by compare-and-set in the transaction that queues
  the records, so projecting twice never queues a record twice.

Every worker-role server runs the worker every 500 ms. A losing server
only loses races; no lease is needed.

## Seals

A seal (`AuditRoot`) covers `first_seq..=last_seq` of an org's chain. It
records:

* the hash of entry `last_seq`;
* the blob holding the sealed entries as JSON lines, written to the org's
  own object store;
* the time;
* the audit key id, and a P-256 signature over all of it.

An org is sealed when its chain has grown and its last seal (or, before
any, its first entry) is `audit.seal_every_secs` old (default one day).
`SealAudit` seals at once.

The audit key is derived from the active token key (like the
[grant key](permissions.md#read-grants)), so it is wrapped by the key
service and rotates with token keys. `TokenKeyRecord.audit_public_key`
holds its public half. `ListAuditKeys` and `GET /public-keys` publish
these keys.

## Verifying

[`verify_chain`](../../crates/rill-policy/src/audit.rs) takes the whole
chain, its seals and the published keys, and reports the first break:

* a missing entry;
* an entry whose `prev` does not match;
* a seal whose head differs, including when an edited entry and every
  link after it are rewritten;
* seals that do not cover consecutive ranges;
* an invalid signature, or a key the instance did not publish.

`rill admin audit verify` runs it against what the server returns.
Entries and seals travel as the exact bytes that were hashed and signed.

## Service and CLI

[`AuditService`](../../crates/rill-api/src/services/audit.rs) is for
administrators of the org (instance administrators for `_instance`):

* `ListAuditEntries`;
* `ListAuditRoots`;
* `SealAudit`;
* `WatchAudit`, which streams entries as they are chained;
* `ListAuditKeys`, which any caller may use.

The CLI is `rill admin audit list/watch/seal/verify/export`. `export
--format ocsf` writes OCSF API Activity events (class 6003) for a SIEM;
`--format json` writes Rill entries.

## Limitations

* Sources are kept for 30 days (repository events) and 7 days (identity
  log). A worker stopped longer than that loses what expired.
* Seals are anchored in the org's own object store; WORM retention (S3
  object lock) is the operator's bucket setting. Anchoring outside Rill is
  not built.
* An entry names the actor principal only, not the session or model that
  acted (the identity log records sessions separately).
* The chain is kept forever; retention and legal hold come with the data
  lifecycle (phase 28).

## Code

* [`rill-types/src/audit/`](../../crates/rill-types/src/audit/mod.rs): records, entries, roots.
* [`rill-store/src/audit/`](../../crates/rill-store/src/audit/mod.rs): queue, chain, cursors, roots.
* [`rill-api/src/audit/`](../../crates/rill-api/src/audit/mod.rs): projection, worker, seals, refusals.
* [`rill-policy/src/audit.rs`](../../crates/rill-policy/src/audit.rs): the audit key and `verify_chain`.
* Tests: `rill-store/tests/conformance/audit.rs` (racing chainers and projectors), `rill-api/tests/grpc/audit.rs` (end to end, tampering, refusals).

---
← Previous: [Signing](signing.md) · [Index](../index.md) · Next: [Data protection](data-protection.md) →

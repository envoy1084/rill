# Human attention

When agents review most changes, people should see only what needs a
person. The `AttentionService` covers four things:

- an inbox per person;
- sampled audits of landings no human approved;
- batch review of changes that make the same edit;
- a daily digest per intent.

## Inbox

`ListInbox` ([attention/inbox.rs](../../crates/domain/review/src/attention/inbox.rs))
gathers what waits on the caller in a repository. It sorts by urgency,
then age:

| Item | When it is the caller's | Urgency |
| --- | --- | --- |
| Escalation | an owner review escalated to them, or to a team they are in ([code owners](ownership.md#escalation)) | 400 + 10 per level |
| Audit | a pending audit assigned to them, a team of theirs, or `owners` when they hold `owner` | 300 |
| Owner review | an open change has an owner set they count for that lacks approvals, and they have not approved | 200, +50 at sensitive risk |
| Intent approval | an intent's scope directory they own still needs approvals | 150 + its linked changes (up to 50) |
| Thread | an unresolved thread is addressed to them | 100 |

`rill inbox` shows these first ("Needs you"). It then lists open reviews
the caller has not voted on, and the caller's own open changes.

## Sampled audits

The `audit-sampler` consumer
([workers/audits.rs](../../crates/server/runtime/src/workers/audits.rs))
looks at every landing. A landing that no human approved on `Review` or
`Owner` is sampled at a rate:

- the rate is the linked intent's `human_sample_permille`, or 20‰;
- it is doubled for normal risk and quadrupled for sensitive and above;
- the draw is deterministic, from the landing's position.

A sampled landing becomes an `Audit` (`audit:<seq>`, indexed in
`audits-pending`). It is assigned to the owner entries of the sets its
paths touch, or to `owners` when no `OWNERS.toml` covers them.

`RecordAudit` (`rill review audit trunk@N --pass|--fail [--revert] -m …`)
may be called by an assignee or a repository owner. A failed audit:
- counts as an escape for every agent that approved the landing
  ([calibration](review-agents.md#calibration)), once per landing;
- with `--revert`, also reverts the landing (that needs `land`).

`ListAudits` (`rill review audits`) shows the pending audits.

## Batch review

[`patch_signature`](../../crates/domain/review/src/attention/similar.rs)
hashes what a version changes without regard to where. It takes the
added and removed lines of every file, trimmed and sorted. Two changes
applying one codemod to different files share a signature.

- **`ListSimilarChanges`** (`rill review batch --intent <id>`) groups an
  intent's changes in review by signature.
- **`BatchVote`** (`--approve <signature>`) votes on each change of a
  group. Each vote is cast with `vote_at` on the version the group was
  computed at, so a change that moved on is reported, not voted. Owner
  approvals are refused in a batch; they stay per change.

## Digests

`GetIntentDigest` (`rill intent digest <id> [--day]`) counts, for one UTC
day, the intent's changes that:

- landed that day;
- of those, were later reverted, blamed or failed in audit;
- of those, were flagged by a reviewer (a negative run);
- of those, were sampled for audit;
- are still in review.

People who ask get it daily ([notifications](../07-runtime/notifications.md#personal-notifications)).

## Limitations

- The inbox scans the repository's reviews and intents on each call.
- Audits are assigned to owner entries, not to a single person in a team.
- A batch groups only changes linked to an intent.

## Code

- [rill-review attention/](../../crates/domain/review/src/attention/mod.rs)
- [rill-runtime workers/audits.rs](../../crates/server/runtime/src/workers/audits.rs)
- [api services/attention/](../../crates/server/api/src/services/attention/mod.rs), [attention.proto](../../crates/foundation/proto/proto/rill/v1/attention.proto)
- [CLI review/attention.rs](../../crates/clients/cli/src/commands/review/attention.rs), [inbox.rs](../../crates/clients/cli/src/commands/review/inbox.rs)
- Test: [api access/attention.rs](../../crates/server/api/tests/grpc/access/attention.rs)

---
← Previous: [Review agents](review-agents.md) · [Index](../index.md) · Next: [Landing](landing.md) →

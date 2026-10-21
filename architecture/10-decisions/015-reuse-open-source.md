# 015 · Reuse mature open source; build only what is Rill's

Status: accepted

## Context

Rill is a version-control system, and its value is in its own design: the
object model, the keyspace, conflicts as data, review and landing, and the
authorization model. Much of the code around that design is generic: caches,
sandboxes, rate limits, protocol servers, file-format and tree-sitter
plumbing, secret rules, retries. Writing it ourselves costs time, and a
small private implementation is less tested than a widely used one. Phase
14 showed the difference: a planned custom structural merger was replaced
by `weave-core` in a fraction of the time.

## Decision

For anything that is not Rill's own design, use a mature, maintained open
source library instead of writing one, and delete the hand-written version
when a library replaces it.

* **Licences.** Rill is Apache-2.0. Link only permissive licences (MIT,
  BSD, Apache-2.0, ISC, Zlib, Unicode, CC0) and MPL-2.0. GPL, LGPL without
  a linking exception, AGPL, SSPL, FSL / source-available, and "free for
  non-commercial use" projects are never linked or shipped; at most they
  are optional external tools an operator installs. `cargo deny` checks
  this in CI.
* **Pinning.** Pre-1.0 or fast-moving crates are pinned to an exact version
  and wrapped behind a Rill trait or module, so an upgrade is a deliberate,
  tested step and a bad release cannot reach production.
* **Testing.** Libraries carry their own tests. Rill tests its integration:
  that the library is wired in, configured as intended, and fails safe. It
  does not re-test the library's algorithm.
* **Kept on purpose.** Hand-written code stays only where it is Rill's
  design, or where no library fits (a persisted format a library would tie
  to its version, FoundationDB-specific transaction logic, behaviour no
  library offers). Each such case says why in its architecture page.

## Consequences

* Less code to maintain; generic behaviour comes from widely used code.
* More dependencies: supply-chain review (`cargo deny`, `cargo audit`)
  matters more, and pinned crates need deliberate upgrades.
* Some behaviour follows the library's choices (weave leaves adjacent
  `match` arms conflicted, for example), documented where it matters.

## Alternatives rejected

* **Build everything in-house:** slower to production, less tested, and
  spends effort away from what makes Rill different.
* **Run external services for generic needs** (an authorization service, a
  webhook service, a job service): each adds an operational dependency and
  a consistency boundary; libraries inside the process are preferred.

## Revisit when

A library stops being maintained, changes to an incompatible licence (as
NativeLink did), or blocks a Rill invariant; then pin, fork or replace it,
recording the reason in the owning page.

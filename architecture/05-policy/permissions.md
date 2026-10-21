# Permissions

Access to a repository is a set of `(object, relation, subject)` tuples stored
in the repository's metadata. Objects are the repository (`repo`) or
directories (`dir:/src/billing`). Subjects are a principal, a team, a
principal kind or everyone. `Authorizer` applies the relation order and
directory inheritance over a versioned snapshot. The invariants:

* a relation granted on a directory applies to its whole subtree;
* a store error denies rather than allowing;
* a check reflects at least every relation change its caller has seen.

## Model

```mermaid
flowchart LR
    O[owner] --> W[writer] --> RV[reviewer] --> R[reader]
```

Each relation implies the ones after it:
[`Relation::satisfied_by`](../../crates/rill-policy/src/rebac/model.rs)
accepts a tuple holding the relation itself or any stronger one. There are no
deny tuples, so checks are monotonic and snapshots cache safely.

Subjects as stored, parsed by `Subject::from_str`:

| Form | Matches |
| --- | --- |
| `principal:<PrincipalId>` | `id == principal.id` |
| `team:<name>` | `name` in `AuthContext::teams` (the user's SCIM or sign-in groups); non-empty, no `/` |
| `kind:human`, `kind:agent`, `kind:service` | `principal.kind` |
| `*` | everyone |

Clients may also name `user:<name>` and `agent:<name>`. These must exist in
the repository's org (outside dev mode) and are stored by principal id.

`dir_object(path)` is `repo` for the root and `dir:/<path>` otherwise.
`inheritance_chain(object)` lists the object, then each ancestor directory,
then `repo`.

## Storage

Tuples are keys with empty values in the repository's metadata
([`keys.rs`](../../crates/rill-store/src/keys.rs)), written in one
transaction by
[`add_relation` and `remove_relation`](../../crates/rill-store/src/repo/relations.rs):

| Key | Purpose |
| --- | --- |
| `(rebac, ns, repo, object, relation, subject)` | tuples by object |
| `(rebacs, ns, repo, subject, object, relation)` | the same tuples by subject (`relations_of_subject`) |
| `(rebacv, ns, repo)` | the relation version, a counter every change bumps |

Each change also writes a `RelationChanged` event. Both calls return the
relation version after the change.

## Snapshot and authorizer

[`Snapshot::load`](../../crates/rill-policy/src/rebac/snapshot.rs) reads the
version, then every tuple, so the tuples reflect at least that version. Tuples
that do not parse are logged and skipped. The snapshot indexes tuples by
object:

* `check` looks up only the objects on the inheritance chain;
* `grants_for(relation, object)` returns the satisfying tuples, nearest
  first, which is what `Check` reports as the reason;
* `granted_dirs` lists the directories where a principal directly holds a
  relation.

[`Authorizer`](../../crates/rill-policy/src/rebac/authorizer.rs) is one per
`RepoContext` and holds one snapshot in a
[`SnapshotCache`](../../crates/rill-policy/src/rebac/cache.rs). The held
snapshot is served while it is at least the requested version and younger
than `DEFAULT_SNAPSHOT_TTL` (30 s, a backstop). It is replaced when:

* a grant or revoke goes through this server's authorizer;
* this server's follower (`workers/follower.rs`) reads a `RelationChanged`
  event written by any server;
* a request asks for a newer version than the one held.

### Consistency across servers

Clients send the relation versions they have seen in the `x-rill-relations`
header: `rp_<hex>=<version>` pairs, separated by commas. `AccessService`
returns a version from every call, and the SDK records versions from grants
and revokes and sends them on every request. A server holding an older
snapshot reloads before checking, so a revoke is honoured on every server
from its author's next request, even before the event log reaches them. A
version the store has not reached is refused with `INVALID_ARGUMENT`.

## Read filtering

[`PathFilter`](../../crates/rill-policy/src/rebac/path_filter.rs) is a cheap
predicate built once per request. `under(roots)` allows paths at or below any
root. A root entry means allow all; an empty list means deny all.
`Permit::read_filter()` returns `allow_all` in dev mode or for an org
administrator, and otherwise the caller's `Reader` directories, narrowed by the token's paths
(a token for another repository, or without `read`, denies all) and the
agent's registered scope. Listings and searches use it so
that names of unreadable directories do not leak: `ListDir` and `Diff`
(`services/repo/trees.rs`), `WorkspaceDiff`, and `Grep`. Point reads such as
file content, history and blame call `permit().read(path)` instead.

## Enforcement

[`Permit`](../../crates/rill-api/src/auth/permit.rs) maps each operation to
a verb, a relation and a path:

| Method | Verb | Relation | Typical callers |
| --- | --- | --- | --- |
| `read(path)` | `Read` | `Reader` | every read RPC, `GetPolicy`, `EvaluateSubmit`, object `GET`/`HEAD` |
| `write(path)` | `Write` | `Writer` | `CreateChangeVersion` per file and subtree path (copy sources need `read`), `ApplyEdits`, workspace snapshot per entry |
| `write(root)` | `Write` | `Writer` | `Rebase`, `Merge`, workspace and intent changes, `SubmitForReview`, `SetStack`, `Abandon`, object `PUT` |
| `review()` | `Review` | `Reviewer` | `Vote`, `Comment`, thread reply and resolve |
| `land()` | `Land` | `Writer` | `RequestLand`, `ReportVerification`, `RevertSeq`, `Bisect` |
| `admin()` | `Admin` | `Owner` | relation changes and listing, `Expand`, checks about others, `SetPolicy`, `IssueToken` without a token |

A check passes the token's capability, then the agent's scope, then either
the tuples or org administration. Instance administrators, and users with the
org's `admin` role presenting an unnarrowed token for it
([`administers_org`](../../crates/rill-api/src/auth/instance.rs)), are owners
of every repository in the org. Dev mode skips the third layer.

A repository with no tuples is closed to everyone but org administrators.
`CreateRepo` makes its creator `owner`, so the creator can grant access, and
an org administrator can recover a repository whose last owner left.

`owns_or_admin(owner)` passes when the caller is the record's owner, and
otherwise requires `admin()`.

## AccessService

[`AccessService`](../../crates/rill-api/src/services/access/mod.rs) names the
repository (`org/name` or a bare name) and a directory path in each tuple.

| RPC | Needs | Does |
| --- | --- | --- |
| `AddRelation`, `RemoveRelation` | `admin()` | change a tuple; returns the repository id and relation version |
| `ListRelations` | `admin()` | tuples, filtered by path, subject or relation |
| `Check` | nothing about oneself; `admin()` about another `user:`/`agent:` | whether the relation is held, the granting tuples nearest first, and whether by org administration |
| `Expand` | `admin()` | every granting tuple on a path, with the org's active users in each team subject |
| `LookupPaths` | as `Check` | the outermost directories where a subject holds a relation (`""` for all) |

The CLI is `rill access grant/revoke/list/check/who/where`
([CLI](../08-clients/cli.md)). The SDK is `Client::access()`
([SDK](../08-clients/sdk.md)).

## Limitations

* `principal:<id>` subjects are not validated; only `user:` and `agent:`
  names are.
* There are no cross-repository roles beyond org administration.
* A snapshot reload reads every tuple of the repository. Changes are rare
  next to checks, but a repository with very many tuples pays for each
  reload.

## Code

* [`rebac/model.rs`](../../crates/rill-policy/src/rebac/model.rs): `Relation`, `Subject`, `dir_object`, `inheritance_chain`.
* [`rebac/snapshot.rs`](../../crates/rill-policy/src/rebac/snapshot.rs): `Tuple`, `Snapshot`.
* [`rebac/cache.rs`](../../crates/rill-policy/src/rebac/cache.rs): `SnapshotCache`, `DEFAULT_SNAPSHOT_TTL`.
* [`rebac/authorizer.rs`](../../crates/rill-policy/src/rebac/authorizer.rs): `Authorizer`; [`rebac/path_filter.rs`](../../crates/rill-policy/src/rebac/path_filter.rs): `PathFilter`.
* [`rill-store/src/repo/relations.rs`](../../crates/rill-store/src/repo/relations.rs): tuples, the subject index and the relation version.
* [`auth/permit.rs`](../../crates/rill-api/src/auth/permit.rs): `Permit`; [`auth/instance.rs`](../../crates/rill-api/src/auth/instance.rs): org administration.
* [`services/access/`](../../crates/rill-api/src/services/access/mod.rs): `AccessService`.
* Tests: [`rebac/tests.rs`](../../crates/rill-policy/src/rebac/tests.rs), `rill-api/tests/grpc/access/` (holes, closed repositories, explanations, relation versions).

---
← Previous: [Token keys](token-keys.md) · [Index](../index.md) · Next: [Risk classification](risk-classification.md) →

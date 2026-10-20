# Web UI

Rill has no browser interface. Every surface is the gRPC API in `rill.v1`,
reached through the SDK by the `rill` CLI and `rill-mcp`. Architecture
principle 10 fixes the constraint for any UI: nothing is possible in a UI that
is not possible through the API, so the UI must be a client of the same gRPC
services, not a second API with its own semantics.

## Current state

There is no web crate, no static asset handling, no HTML route. The HTTP
listener ([objects/mod.rs](../../crates/rill-server/src/objects/mod.rs))
serves `/o/{repo}/{kind}/{hash}`, `/o/{repo}/batch`, `/healthz` and `/readyz`
only. The gRPC listener ([serve.rs](../../crates/rill-server/src/serve.rs))
serves six tonic services behind `AuthLayer`; `tonic-web` is not a dependency
and no CORS layer exists, so a browser cannot call it.

What the API already offers a review UI, by service:

| Need | RPC | Gap for a UI |
| --- | --- | --- |
| Review list | `ReviewService.ListReviews(status[], owner, intent, limit, page_token)` | Returns full `ReviewState` (every vote, every thread with every comment) per row; no summary shape, no sort order parameter, no filter by reviewer or label value. |
| Review detail | `GetReview` → `ReviewState` + `labels` | `ReviewState.versions` is a list of hashes; the UI needs author, message and timestamps per version (`GetChangeInfo` per hash, N round-trips). |
| Diff | `RepoService.Diff(base, target, paths, format, context_lines, limit)` | Whole-file hunks; no per-file pagination, no "load more context", no syntax token stream. `FileDiff.conflict` carries `MergeTerms` for conflicted paths. |
| Interdiff between versions | `rill_review::lifecycle::interdiff` exists server-side for vote copying | Not exposed as an RPC; the UI would call `Diff` with two version roots, which diffs the trees, not the changes relative to their moved bases. |
| Threads and comments | `Comment`, `ReplyThread`, `ResolveThread`; `Thread { id, anchor, comments, resolved, to }` | Anchors are `Lines { version, path, start, end, context_hash }`; the UI needs the anchor re-mapped onto the version it is displaying (`threads/anchor.rs` does this on `VersionAdded`, but a thread whose context vanished stays on its original version with no "outdated" marker in the proto). |
| Votes | `Vote`, `GetReview.labels` | Fine. |
| Submit evaluation | `EvaluateSubmit` | Fine; returns the Cedar decision and the facts. |
| Land status | `RequestLand`, `GetLandStatus(ticket)` | A ticket is returned to the caller that requested landing; the review page for another viewer has to find the in-flight ticket through `LanderService.GetTrunkStatus.in_flight`. |
| Trunk health | `GetTrunkStatus { head_seq, head_hash, in_flight, recent, last_verified_seq }`, `ListLandings`, `GetVerification` | Enough for a status bar; no per-seq verification detail stream. |
| Live updates | `WatchReview(change_id)`, `WatchTrunk`, `WatchRefs`, `WatchLog` | Server-streaming gRPC; works over grpc-web with a streaming-capable transport. |
| Identity | `AdminService.WhoAmI` | Fine. |
| Files and browsing | `GetTree`, `ListDir`, `ResolvePath`, `GetBlob` (streamed), `Blame`, `PathHistory`, `Grep` | Fine for a file viewer; `GetBlob` streams raw bytes, no syntax highlighting (client-side). |

Authentication ([auth/](../../crates/rill-server/src/auth/)): a bearer token
in `authorization` metadata (gRPC) or the `Authorization` header (HTTP),
resolved by `authenticate` to an `AuthContext` from either the static token
list or a Biscuit token verified against the root key
([config.rs](../../crates/rill-server/src/config.rs) `AuthConfig`). There is
no session, cookie, OIDC or CSRF handling.

## Why it blocks production

1. **Review is unusable for humans.** `rill review show` prints state; reading
   a 40-file diff with threads in a terminal does not scale to a team.
   Reviewers who cannot see the diff do not vote, and `EvaluateSubmit` then
   blocks landing. The humans/agents split in principle 10 presumes an
   ergonomic surface exists.
2. **Trunk incidents are invisible.** `LandEvent::Verification { green: false
   }`, `Culprit`, `Reverted` are published on the bus and visible to
   `WatchTrunk` subscribers only. An on-call engineer has no page that shows
   "trunk red since seq 1832, bisect running".
3. **Agents are unobservable.** Agents vote and comment through MCP with
   signed provenance, but nobody can read the resulting threads without the
   CLI. A review process where agent findings cannot be triaged by a human is
   one nobody will trust.
4. **A second API would fork semantics.** If a UI grows a REST layer with its
   own list/diff endpoints, principle 10 is violated the first time the REST
   layer gains a filter the gRPC API lacks.
5. **Browser auth does not exist.** Pasting a Biscuit token into
   `localStorage` is the only option, which leaks long-lived capability tokens
   to any XSS and gives no logout.

## Requirements

* R1. A browser can list reviews (filtered by status, owner, label value,
  reviewer), open one, read the diff of the latest version and of any version
  pair, see threads anchored at the lines they refer to on the displayed
  version, vote, comment, reply, resolve, evaluate, request landing and watch
  the land ticket. Test: Playwright flow against `rill-server --dev-mode`.
* R2. A trunk page shows head seq, in-flight landings, last verified seq, the
  last N landings with verification state, culprit and revert events, updated
  live.
* R3. Every UI action is a call to an existing `rill.v1` RPC over grpc-web or
  Connect; the server exposes no UI-only endpoints except static assets and
  the session exchange.
* R4. Browser sessions are short-lived credentials bound to a cookie, minted
  from a human login (OIDC) and exchangeable for a Biscuit scoped to the
  session; no long-lived token is stored in the browser. Logout revokes the
  session.
* R5. Permission filtering is identical to the CLI: a user who lacks `Reader`
  on a path sees the same `PermissionDenied` the SDK sees, and path-filtered
  `Grep` results are filtered the same way.
* R6. Reads needed by a review page complete in a bounded number of
  round-trips (one list call, one review call, one diff call per version pair,
  one change-info batch call).
* R7. The UI works with the server's lossy event bus: on `Lagged` or reconnect
  it re-reads state rather than relying on having seen every event.

## Design direction

### Recommended: SPA served by `rill-server`, speaking Connect/grpc-web to the same services

**Transport.** Add `tonic-web` (`GrpcWebLayer`) and a CORS layer to the gRPC
server, so the existing services answer `application/grpc-web+proto` from a
browser, including server-streaming for `Watch*`. Generate TypeScript clients
from `rill-proto/proto/rill/v1/*.proto` with `protoc-gen-connect-es`; Connect
clients speak grpc-web to a tonic server without an adapter. Streams map to
async iterators. The `AuthLayer` is unchanged: the browser sends
`authorization: Bearer <session-biscuit>`.

**Serving.** A `rill-web` crate (or a feature of `rill-server`) embeds the
built SPA with `rust-embed` and serves it from the HTTP listener under `/ui/`,
falling back to `index.html` for client routes. The gRPC listener and HTTP
listener stay separate processes/ports in config, with a documented
reverse-proxy layout that puts both behind one origin so cookies and CORS are
simple.

**SPA, not SSR.** The data is behind authenticated gRPC calls and changes
live; there is no SEO or first-paint-without-JS requirement. SSR would require
the server to hold a gRPC client to itself and re-implement auth for the
render path. A SPA with route-level code splitting keeps the server a pure API
host. Framework: SolidJS or React with Connect's query integration; the choice
is not architectural.

**Diff rendering.** `Diff` with `DiffFormat::STRUCTURED` gives `DiffHunk`s per
file. The UI renders them with client-side syntax highlighting
(tree-sitter-wasm or Shiki). Large diffs: request `limit` files, then per-file
`Diff` with `paths=[one]` on expand. `FileDiff.conflict` (`MergeTerms`) is
rendered as the same snapshot-plus-diff layout `merge/materialize.rs` uses for
markers, so the UI and the working copy show a conflict the same way.

**Threads.** Anchors are shown on the version they were made on; when the user
views another version the UI asks the server where the anchor lands by calling
`GetReview` after `VersionAdded` (the server already relocated anchors with
`find_context`). Threads whose anchor could not be relocated need an
`outdated: bool` on `Thread`; that is an API gap to fill, not a UI heuristic.

**Live updates.** One `WatchReview` stream per open review page, one
`WatchTrunk` per trunk page. Each event triggers a re-fetch of the affected
object (`GetReview`, `GetTrunkStatus`), mirroring how
[watch.rs](../../crates/rill-server/src/services/repo/watch.rs) itself treats
bus events as wake-ups. Reconnect with backoff; R7 is then free.

**Auth for browsers.**

```mermaid
sequenceDiagram
  participant B as Browser
  participant W as /ui + /auth (HTTP listener)
  participant I as OIDC provider
  participant G as gRPC (AuthLayer)
  B->>W: GET /auth/login
  W-->>B: 302 to I (PKCE)
  B->>I: login
  I-->>B: 302 /auth/callback?code
  B->>W: GET /auth/callback
  W->>I: exchange code
  W->>W: mint Biscuit (principal, ttl 12h, session id) via issuer
  W-->>B: Set-Cookie rill_session=<opaque>; HttpOnly; SameSite=Lax
  B->>W: POST /auth/token (cookie)
  W-->>B: short-lived Biscuit (15 min) in body
  B->>G: rpc with authorization: Bearer <biscuit>
  G->>G: authenticate (existing path)
```

Sessions are rows in a new `MetaStore` keyspace (`sess/<id>` → principal,
expiry, revoked). The browser never holds the session Biscuit; it holds a
15-minute Biscuit attenuated from it, refreshed from the cookie. Revocation is
a delete of the session row; the attenuated token expires within minutes.
`AuthConfig` gains an `Oidc { issuer, client_id, ... }` variant alongside
`Static` and `Biscuit`; `build_issuer` already exists for minting.

**API gaps to fill (server-side, small).**

| Gap | Change |
| --- | --- |
| Review list payload | `ListReviewsRequest.summary = true` returns `ReviewSummary { change_id, owner, status, latest_version, title (first message line), vote_totals, open_threads, updated_at }`; add `sort` and `reviewer`/`label` filters. |
| Version metadata | `GetChangeVersions(hashes[])` already exists; use it in batch. |
| Interdiff | `ReviewService.Interdiff(change_id, from_version, to_version)` exposing `lifecycle::interdiff`. |
| Outdated anchors | `Thread.outdated` set when `find_context` fails for the latest version. |
| Land ticket discovery | `GetReview` includes `land_ticket` when a landing is in flight for the change. |
| Verification detail | `GetVerification(seq)` already returns `VerificationResult`; add step results (`ActionResult` references) so a red trunk links to logs. |

### Alternatives considered

| Option | Trade-off |
| --- | --- |
| REST/JSON gateway (grpc-gateway style) in front of the services | Second API surface to keep in sync; JSON mapping of `bytes` ids is awkward. Rejected by principle 10. |
| SSR with htmx, server holds an SDK client | Zero client build, but the server becomes a client of itself, duplicating the auth path, and live updates need SSE plumbing that reimplements `Watch*`. Good for a status page only. |
| Separate `rill-web` process | Cleaner boundary, but needs its own deploy and a shared-origin story. The embedded SPA can be split out later without API change. |
| Long-lived Biscuit in `localStorage` | No logout, XSS exfiltration gives a durable credential. Rejected. |

## Work breakdown

1. **S** Add `tonic-web` and CORS layers to `serve_grpc`, behind `ui.enabled`;
   verify `Watch*` streams from a browser.
2. **S** Generate Connect-ES clients from the protos in CI; publish as a
   workspace package.
3. **M** API gaps: `ReviewSummary`, list filters/sort, `Thread.outdated`,
   `GetReview.land_ticket`, `Interdiff` RPC. SDK and CLI pick these up too.
4. **M** `rill-web` crate: embedded static assets on the HTTP listener,
   `/ui/*` fallback routing.
5. **L** Review list and review page: diff viewer (structured hunks, conflict
   rendering, per-file expansion), threads with anchor placement, votes,
   evaluate, request land, ticket watch.
6. **M** Trunk page: status, landings, verification, culprit/revert timeline,
   live via `WatchTrunk`.
7. **L** Browser auth: `AuthConfig::Oidc`, `/auth/*` routes, session keyspace,
   attenuated token exchange, logout, tests with a mock OIDC provider.
8. **M** File browser: tree, blob view with highlighting, blame, path history,
   grep.
9. **S** Playwright end-to-end suite against `rill-server` in dev mode.

## Risks and open questions

* grpc-web server streaming through some proxies (older nginx configs,
  Cloudflare without HTTP/2 to origin) is buffered; Connect's streaming over
  HTTP/1.1 chunked works where grpc-web does not. Document the proxy
  requirements.
* `ListReviews` currently materialises whole `ReviewState` records; a
  repository with thousands of open reviews makes the list call large before
  `summary` lands. Step 3 is on the critical path.
* Thread anchor relocation runs on `VersionAdded`; a thread opened on version
  3 and viewed on version 1 has no anchor there. The UI should show it as
  "from a later version" rather than inventing a position.
* OIDC group claims versus ReBAC teams: `StaticToken.teams` is the only team
  source today; the OIDC callback needs a claim-to-team mapping or an admin
  RPC to set team membership per principal.
* Open: does the UI need write access to workspaces (edit in browser)? Out of
  scope for a review UI; `WorkspaceService` exists if wanted.
* Open: one origin or two? Serving gRPC and HTTP on separate ports is fine
  behind a proxy, but local development needs CORS on the gRPC port, which the
  dev-mode default should enable.

## References

Source: [serve.rs](../../crates/rill-server/src/serve.rs),
[objects/mod.rs](../../crates/rill-server/src/objects/mod.rs),
[auth/http.rs](../../crates/rill-server/src/auth/http.rs),
[auth/context.rs](../../crates/rill-server/src/auth/context.rs),
[config.rs](../../crates/rill-server/src/config.rs),
[services/review.rs](../../crates/rill-server/src/services/review.rs),
[services/repo/watch.rs](../../crates/rill-server/src/services/repo/watch.rs),
[review.proto](../../crates/rill-proto/proto/rill/v1/review.proto),
[lander.proto](../../crates/rill-proto/proto/rill/v1/lander.proto),
[types.proto](../../crates/rill-proto/proto/rill/v1/types.proto),
[rill-review/src/threads/anchor.rs](../../crates/rill-review/src/threads/anchor.rs),
[rill-review/src/lifecycle/interdiff.rs](../../crates/rill-review/src/lifecycle/interdiff.rs).

Prior art: Gerrit (change list, patchset interdiff, label votes, comment
anchoring across patchsets), Phabricator Differential (stacked revisions,
inline comments), GitHub pull request review UI and its "outdated" comment
state, ReviewBoard, Sapling's ReviewStack (a pure-client UI over an existing
API), Connect-ES and `tonic-web` for browser gRPC, Biscuit attenuation for
short-lived browser tokens, the BFF (backend-for-frontend) cookie-session
pattern for OIDC in SPAs, Shiki and tree-sitter-wasm for client-side
highlighting.

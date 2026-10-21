# SDK

`rill-sdk` is the typed Rust client for the `rill.v1` gRPC API. The CLI,
the MCP server and agents all use it. It returns server results as they are:
a gRPC status becomes `Error::Status` with its code unchanged. The only local
state it manages is a `.rill/` checkout.

## Shape

```mermaid
flowchart LR
    Cfg[ClientConfig] --> Cl[Client<br/>Channel + AuthInterceptor]
    Cl --> Repo[RepoClient]
    Cl --> Admin[AdminClient]
    Repo --> WS[WorkspaceClient]
    Repo --> Rv[ReviewClient]
    Repo --> In[IntentClient]
    Repo --> La[LanderClient]
    Repo --> St[StackClient]
    Repo --> Co[local::Checkout]
```

A `Client` is one HTTP/2 `Channel` plus an `AuthInterceptor`. The
interceptor adds `authorization: Bearer <token>` when a token is set and
nothing otherwise. `Client::repo(name_or_id)` returns a `RepoClient`, which
puts the repository on every request and hands out the per-service clients.
Inputs and outputs are `rill-types` values where one exists. Otherwise they
are small structs in `types/`.

`Client::connect` dials eagerly, so a bad endpoint fails at once.
`connect_lazy` connects on first use; the CLI and MCP server use it so
`--help`, `initialize` and `tools/list` work with no server running.
`with_channel` wraps an existing `Channel` for tests. The endpoint uses a
10 s connect timeout, 30 s TCP and HTTP/2 keepalives, and the per-request
`timeout` from the config. Streams ignore that timeout.

## Configuration

`ClientConfig::new(endpoint)` has no token, a 30 s timeout and user agent
`rill-sdk/<version>`. `ClientConfig::discover` tries `from_env` first:
`RILL_ENDPOINT` (required) and `RILL_TOKEN` (empty means none). It then
falls back to `load(default_path())`, a TOML file with `[server] endpoint`
and optional `token` and `timeout_secs`. The path is
`dirs::config_dir()/rill/config.toml`: `~/.config` on Linux,
`~/Library/Application Support` on macOS.

For `https://` endpoints the channel uses TLS (`TlsOptions`): the server's
certificate is checked against the system's roots plus `ca_file`, and
`client_cert` / `client_key` are presented to servers that require them.
`ClientConfig::new` takes them from `RILL_CA_FILE`, `RILL_CLIENT_CERT` and
`RILL_CLIENT_KEY`; the config file's `[server]` section may set
`ca_file`, `client_cert` and `client_key`.

## Repository API

Repositories are named `org/name`, or by a bare name in the default org;
`RepoInfo.org` names the org. `AdminClient::create_org` and `list_orgs`
manage orgs (`OrgInfo`). Write methods send an `operation_id`, so a
retried call returns the first call's result (see
[API surface](../07-runtime/api-surface.md#retries)).

`RepoClient` covers repository info, history (`log`, `log_page`,
`path_history`, `blame`), trees and files (`list_dir`, `read_file`), blobs,
diff and ancestry, refs and trunk, `grep`, derived data and `watch_*`
streams. Each module under `repo/` wraps one group of RPCs.
`watch_log(WatchStart)` starts `Now`, from the `Beginning` or `After` a
`Versionstamp` cursor, and yields `LogEntry { cursor, info }`, so a client
can stop and resume without missing or repeating a version.
`watch_events(WatchStart, EventQuery { kinds, change, ref_prefix })`
streams the event log the same way, yielding `LoggedEvent { cursor, event
}` with the `rill_types::Event`.

`Root` (`types/root.rs`) names a point in history: `Ref`, `Version`, `Tree`
or `Seq`. It parses from `trunk`, `trunk@N`, `tree:<hex>`, a 64-hex version
hash, or a ref name. `LogStart` is the same without `Tree`.

`get_blob` returns `ContentRef::Inline` bytes with no round trip. For other
content it joins the `BlobChunk` stream into one `Bytes`. `log_page` returns
a `next_start` cursor hash. History pages by cursor, not by token.

## Write path

```mermaid
sequenceDiagram
    participant A as Caller
    participant R as RepoClient::apply_edits
    participant S as Server
    A->>R: EditBase, Vec<Edit>, ApplyOptions
    loop each Edit::Set
        alt len <= INLINE_THRESHOLD (256 B)
            R->>R: ContentRef::Inline
        else
            R->>S: PutBlob (256 KiB frames)
            S-->>R: ContentRef
        end
    end
    R->>S: ApplyEdits (edits, provenance, intent, change_id)
    S-->>R: VersionResult {hash, version}
```

`Edit` is `Set {path, bytes, kind}`, `Delete {path}` or `Patch {path,
unified}`. `EditBase::Root(root)` creates a change on top of a root, and
`EditBase::Change(id)` amends that change's current version. `rebase`,
`merge` and `resolve` also return `VersionResult`. A conflict is data in the
version (`conflicted_paths()`), not an error. `create_change_version` sends
a fully built `ChangeVersion` with detached signatures. Every write has a
`_with_op` twin that takes an `OperationId`; the plain form mints a fresh one.

`EventStream<T>` is a boxed `Stream<Item = Result<T>>`. Each wire message is
converted with `TryFrom`. A `Status` becomes one `Error::Status` item, and
the stream ends after it. List RPCs return `Page<T> {items,
next_page_token}`.

## Local checkouts

`local/` is a minimal working copy for the CLI. Agents do not need one.

- `.rill/checkout.toml` holds `CheckoutMeta`: endpoint, repo, workspace,
  base, base_version, working_change and sparse profile.
- `.rill/manifest.toml` maps each path to a `ManifestEntry` (content hash,
  local hash, size, mtime, kind, conflict flag). Both files are written
  atomically.
- `Checkout::create` lists the tree one `ListDir` at a time, never entering
  a directory outside the `SparseProfile`, and writes the files.
- `status` treats a file as unchanged when size and mtime match the
  manifest, and hashes it otherwise. `.rillignore` (gitignore syntax) hides
  untracked files only.
- `snapshot` turns status into edits and calls `apply_edits` with
  `EditBase::Root(base)` and `change_id = working_change`, so later
  snapshots amend the same change. An unresolved conflict is an
  `Error::Checkout`.
- `update(new_root)` classifies each path by upstream and local change. If
  both sides changed, it runs `rill_core::merge::merge_file`. A conflict is
  written as marker text and flagged in the manifest. It returns
  `UpdateReport {updated, removed, merged, conflicts}`. Filesystem work
  runs under `spawn_blocking`.

## Stacks

`StackClient` (`stack.rs`) combines review and repo calls. `submit_stack`
submits each change, then calls `set_stack`. `sync_stack` rebases bottom-up,
each change onto the previous result, and reports conflicts instead of
raising them. `land_stack` requests a land for each change and waits for it
before the next. `await_land` polls land status with `Backoff` (250 ms
initial, factor 1.6, 5 s max). It returns `Error::LandFailed` on a failed or
cancelled land and `Error::Timeout` when the deadline passes.

## Errors

`Error` variants: `Status {code, message}`, `Transport`, `Convert`,
`Invalid`, `Config`, `Io`, `Timeout`, `Conflict {change_id, hash, paths}`,
`Checkout` and `LandFailed {ticket, status, message}`. Callers branch on
`Error::code()` and `is_not_found()`. See
[api surface](../07-runtime/api-surface.md) for status codes.

## Limitations

- `put_blob` never sets `expected_hash`. Integrity depends on the server
  hashing the bytes it receives.
- `get_blob_range` allocates the whole requested range before returning.
- A checkout has no rename detection. A moved file shows as `Deleted` plus
  `Added`. A symlink's content is its target string.
- When `RILL_ENDPOINT` is set, `discover` ignores the config file entirely.
- The server always returns `next_page_token: None`, so a page holds the
  whole limit-bounded result.
- No retry or reconnect policy beyond what the tonic channel does.
- `RepoClient::grep` has no fallback; the tree walk lives in [mcp](mcp.md).

## Code

- [lib.rs](../../crates/rill-sdk/src/lib.rs), [client.rs](../../crates/rill-sdk/src/client.rs), [config.rs](../../crates/rill-sdk/src/config.rs), [error.rs](../../crates/rill-sdk/src/error.rs), [stream.rs](../../crates/rill-sdk/src/stream.rs)
- [repo/mod.rs](../../crates/rill-sdk/src/repo/mod.rs), [repo/blobs.rs](../../crates/rill-sdk/src/repo/blobs.rs), [repo/write.rs](../../crates/rill-sdk/src/repo/write.rs), [repo/history.rs](../../crates/rill-sdk/src/repo/history.rs), [repo/watch.rs](../../crates/rill-sdk/src/repo/watch.rs)
- [types/root.rs](../../crates/rill-sdk/src/types/root.rs), [types/edit.rs](../../crates/rill-sdk/src/types/edit.rs), [types/page.rs](../../crates/rill-sdk/src/types/page.rs)
- [local/checkout.rs](../../crates/rill-sdk/src/local/checkout.rs), [local/meta.rs](../../crates/rill-sdk/src/local/meta.rs), [local/status.rs](../../crates/rill-sdk/src/local/status.rs), [local/update.rs](../../crates/rill-sdk/src/local/update.rs), [local/walk.rs](../../crates/rill-sdk/src/local/walk.rs)
- [stack.rs](../../crates/rill-sdk/src/stack.rs), [review.rs](../../crates/rill-sdk/src/review.rs), [workspace.rs](../../crates/rill-sdk/src/workspace.rs), [lander.rs](../../crates/rill-sdk/src/lander.rs), [admin.rs](../../crates/rill-sdk/src/admin.rs)

---
← Previous: [Observability](../07-runtime/observability.md) · [Index](../index.md) · Next: [CLI](cli.md) →

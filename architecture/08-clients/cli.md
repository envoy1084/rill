# CLI

`rill-cli` builds two binaries: `rill`, the user command, and `rill-api`,
the server process. Both are thin `main`s over the `rill_cli` library, so
tests run the CLI in-process with `run_cli_with_args`. Every command builds
one JSON value with `rill_mcp::json`, the same encoding the MCP tools return.
It prints that value under `--json` and a human view of it otherwise.

## Flow

```mermaid
flowchart LR
    Args[argv] --> Clap[Cli::try_parse_from]
    Clap -->|help / version| Ok0[stdout, exit 0]
    Clap -->|parse error| E2[stderr, exit 2]
    Clap --> Ctx[Ctx::new]
    Ctx --> Run[commands::run]
    Run -->|Ok| Emit[ctx.emit: JSON or human]
    Run -->|CliError| Rep[report_error, exit::code_for]
    Ctx -.lazy.-> Client[Client::connect_lazy]
    Ctx -.lazy.-> Co[Checkout::find]
```

`run_cli` reads `-v` and `-q` before clap runs, so logging also covers
parsing. Logs go to stderr at `warn` by default, and `RUST_LOG` overrides
the level. Colour is on only when stdout is a terminal, `NO_COLOR` is unset,
and neither `--no-color` nor `--json` is given.

## Commands

Global options work in any position: `--endpoint`, `--token`, `--repo`,
`--json`, `-C/--cwd`, `-v` (repeatable), `-q`, `--no-color`.

| Group | Commands |
| --- | --- |
| Repositories | `repo list`, `repo create [--windows-safe]`, `repo info`; names are `org/name` or a name in the default org |
| Orgs | `org list`, `org create <name> [--description]` |
| Checkout | `checkout`, `status`, `diff` |
| History | `log`, `blame`, `show`, `ls`, `cat`, `grep` |
| Changes | `new`, `describe`, `amend`, `sync`, `resolve`, `undo` |
| Stacks | `stack show`, `stack sync`, `stack submit`, `stack land` |
| Review | `submit`, `review show/list/vote/comment/threads/resolve-thread/evaluate/abandon` |
| Landing | `land [--wait] [--timeout 300] [--no-sync]`, `trunk`, `landings`, `verify`, `revert`, `bisect` |
| Intents | `intent create/list/show/update/link/similar` |
| Workspaces, refs | `ws create/list/show/handoff/ops/delete`, `ref list/show/set/delete` |
| Sign-in | `login [--org] [--provider]` (device flow through the org's identity provider), `logout` |
| Services | `dev`, `api [--roles]`, `cache --origin --dir --max-bytes` ([cache tier](../07-runtime/cache.md)) |
| Access | `access grant/revoke <subject> <relation> [path]`, `access list [--path] [--subject] [--relation]`, `access check <relation> [path] [--as]`, `access who <relation> [path]`, `access where <relation> [--as]` |
| Admin | `admin whoami`, `admin token issue`, `admin policy get/set`, `admin health`, `admin idp add/list/remove` (OIDC, or SAML with `--saml-metadata`), `admin user list/suspend/activate/roles`, `admin key list/rotate`, `admin revoke token/session`, `admin agent register/list/suspend/activate`, `admin trust add/list/remove` (workload issuers), `admin kill --agent/--session/--sponsor/--model`, `admin scim create/list/revoke`, `admin audit list/watch/seal/verify/export`, `admin protect redact/approve/redactions/release/rotate-key`, `admin secret set/list/delete` (the value from standard input or `--from-file`, never an argument) |
| Other | `import git` (through the server, or `--storage-dir`), `export git` (mirror trunk to a git branch), `watch trunk/refs/log/events` (`log` and `events` take `--after <cursor>` or `--from-start`; `events` filters with `--kind` (repeatable), `--change` and `--ref-prefix`), `completions <shell>` |

Each group lives in `commands/<group>.rs`. `commands/parse.rs` holds the
shared positional parsers: `root_arg` (defaults to trunk), `seq_arg` (`7` or
`trunk@7`) and `change_ref` (`ch_<hex>` or a version hash).

Behaviours worth knowing:

- `rill new` with a previous working change: if that change has landed, the
  command clears it and starts fresh. Otherwise it updates the checkout onto
  the previous change's current version and snapshots a new change stacked on
  it. Only edits made since the last snapshot go into the new change.
- `rill new` and `rill sync` change `working_change` through
  `checkout_meta::set_working_change`. This is the only place the CLI edits
  `.rill/checkout.toml` directly.
- `rill import git <path> --repo R` imports through the server the client
  is configured for; `--storage-dir DIR` (or `RILL_STORAGE_DIR`) opens
  `LocalStores` directly instead. `rill export git <dir>` mirrors trunk into
  a git branch. See [git import and export](../06-services/git-import.md).
- Every change the CLI creates has `Provenance {tool: "rill-cli"}`.
- `rill login` lists the org's providers through `SessionService`, runs the
  device flow against the chosen one's CLI client, prints the code and URL
  on stderr, exchanges the ID token for a Rill session and stores it
  (`credentials.rs`): in the OS keychain (service `rill`, account = the
  endpoint), or a 0600 JSON file when the keychain fails or
  `RILL_CREDENTIALS_FILE` names one (`~/.config/rill/credentials.json` by
  default). Later commands use the stored session, which refreshes itself
  and saves each rotated refresh token. `rill logout` ends the session on
  the server and forgets it.
- Under `RILL_WORKLOAD_TOKEN_FILE` with no token given, every command except
  `dev`, `api`, `login`, `logout` and `completions` connects first,
  exchanging the workload's JWT for an agent token
  (`Ctx::resolve_workload`).

## Context resolution

`Ctx` (`context.rs`) resolves each setting lazily, at most once per
invocation.

| Setting | Order |
| --- | --- |
| endpoint | `--endpoint`, then the checkout's `endpoint`, then `ClientConfig::discover` (`RILL_ENDPOINT`, then the config file) |
| token | `--token`, then `RILL_TOKEN`, then the config file's token (only when the file also supplied the endpoint), then the session `rill login` stored for the endpoint |
| repository | `--repo`, then the checkout's `repo`, then `RILL_REPO`, else a usage error |
| checkout | `Checkout::find` walks up from `-C` or the process cwd to the nearest `.rill/` |

The client uses `connect_lazy` with user agent `rill-cli/<version>`.
Commands that never call the server never dial it.

## Output, errors and exit codes

`Ctx::emit(value, human)` pretty-prints the JSON under `--json`. Otherwise it
runs the human renderer against `Out`, which provides `line`, `kv`, `table`
(`comfy-table`) and style helpers. `watch` prints one compact JSON object per
line through `emit_json_line`.

Errors go to stderr. With `--json` an error is printed as
`{"error": {code, message, exit}}`. `code` comes from `CliError::code`. For
SDK errors it is `rill_mcp::error::sdk_code`, the snake_case gRPC code.

| Exit | Meaning |
| --- | --- |
| 0 | success, including `--help` and `--version` |
| 1 | failure: server error, not found, permission denied, transport, I/O, storage |
| 2 | usage: parse error, `CliError::Usage`, `CliError::Invalid`, SDK `Config` error |
| 3 | needs attention: conflict, `LandFailed`, SDK `Conflict`, status `ABORTED` or `FAILED_PRECONDITION` |

Scripts and agents can rely on these codes. Exit 3 means "read the output
and decide".

## Running the service

The service runs from the same binary (`server/mod.rs`):

* `rill dev [--data DIR | --memory] [--listen-grpc ADDR] [--listen-http ADDR]`
  runs every role in one process with `dev_mode` on, storage in
  `~/.rill/dev` by default, and ports 7470 / 7471 (`DEV_GRPC_PORT` /
  `DEV_HTTP_PORT`). Verification is on, with two unisolated in-process
  execution workers (`DEV_LOCAL_WORKERS`) running `.rill/verify.toml`.
* `rill api [--config PATH] [--listen-grpc ADDR] [--listen-http ADDR]
  [--storage memory|local:<dir>] [--roles api,workers]` loads the file and
  `RILL_*` environment through `ServerConfig::load`, then applies the
  flags. `rill api init-config [--out PATH] [--force]` prints an annotated
  `ServerConfig::default()`.
* `rill api [--config PATH] mint-token <human:|agent:|service:>NAME
  [--org ORG] [--ttl SECS]` signs a token with the instance's key, reading
  the store and key service from the configuration like a server would. It
  is how the first administrator (and staging) gets a token; the lifetime
  defaults to the longest the kind allows.

Both log at `info` unless `-q` or `RUST_LOG` say otherwise. Ctrl-C cancels
the root `CancellationToken`. See
[configuration](../07-runtime/configuration.md).

## Limitations

- If checkout metadata fails to parse, the CLI logs a warning and acts as if
  it is outside a checkout.
- `rill import git` prints only its final result, with no progress output.
- `rill api` has no flags for S3 storage or the key service. Those come
  from the config file or the environment.
- The `--endpoint` help text gives `~/.config/rill/config.toml` as the config
  path. On macOS the SDK reads `~/Library/Application Support/rill/config.toml`.

## Code

- [lib.rs](../../crates/clients/cli/src/lib.rs), [cli.rs](../../crates/clients/cli/src/cli.rs), [context.rs](../../crates/clients/cli/src/context.rs), [error.rs](../../crates/clients/cli/src/error.rs), [exit.rs](../../crates/clients/cli/src/exit.rs), [checkout_meta.rs](../../crates/clients/cli/src/checkout_meta.rs)
- [commands/mod.rs](../../crates/clients/cli/src/commands/mod.rs), [commands/parse.rs](../../crates/clients/cli/src/commands/parse.rs), [commands/change/mod.rs](../../crates/clients/cli/src/commands/change/mod.rs), [commands/import/](../../crates/clients/cli/src/commands/import/mod.rs), [commands/export.rs](../../crates/clients/cli/src/commands/export.rs)
- [output/mod.rs](../../crates/clients/cli/src/output/mod.rs), [output/table.rs](../../crates/clients/cli/src/output/table.rs), [output/format.rs](../../crates/clients/cli/src/output/format.rs)
- [server/mod.rs](../../crates/clients/cli/src/server/mod.rs), [server/template.rs](../../crates/clients/cli/src/server/template.rs)
- [bin/rill.rs](../../crates/clients/cli/src/bin/rill.rs)

---
← Previous: [SDK](sdk.md) · [Index](../index.md) · Next: [MCP server](mcp.md) →

# Getting started

## Build

```
cargo build --release --workspace
export PATH="$PWD/target/release:$PATH"
```

## A local server

```
rill dev
```

`rill dev` runs every role in one process, keeps data in `~/.rill/dev`
(`--data <dir>` elsewhere, `--memory` for nothing at all), maps
unauthenticated requests to a `dev` human principal, and listens on
`127.0.0.1:7470` (gRPC) and `127.0.0.1:7471` (object protocol and health).
For a real server:

```
rill api init-config > rill.toml         # edit storage, identity.keys, identity.admins
rill api --config rill.toml
rill api --config rill.toml mint-token human:alice   # the first token
```

## First repository

```
export RILL_ENDPOINT=http://127.0.0.1:7470
rill repo create demo
rill checkout demo ./demo
cd demo
```

The checkout writes a `.rill/` directory with the endpoint, repo, server-side
workspace id, base version and a file manifest. Everything `rill` does from
inside the checkout is an API call; the server owns the history.

## The change loop

```
echo 'fn main() { println!("hi"); }' > src/main.rs
rill status                     # A src/main.rs
rill new -m "add main"          # snapshot into a new change
rill show                       # change id, version hash, files
rill submit                     # open a review
rill review evaluate <change>   # why it is or is not landable
rill review vote <change> Review 2
rill review vote <change> Verified 1
rill land --wait                # trunk@1
```

Edit again and `rill amend` to produce a new version of the same change
(same change id, new hash; review votes survive trivial rebases). `rill new`
on top of an unlanded change starts a stack; `rill stack show`, `rill stack
sync`, `rill stack land`.

When trunk moves under you, `rill sync` rebases the working change onto the
new head. A conflict does not stop anything: the files get conflict markers,
`rill status` shows `C`, and `rill resolve <path>` records the resolution
after you edit.

## For agents

```
rill-mcp --endpoint http://127.0.0.1:7470 --repo demo
```

exposes 27 tools over stdio (MCP 2025-06-18). The important ones:
`rill_read_file`, `rill_grep`, `rill_diff`, `rill_apply_edits` (set, delete
or patch files and create a change in one call, no checkout), `rill_submit`,
`rill_request_land`, `rill_land_status`, `rill_intent_*`. Pass `model` and
`session` in `rill_apply_edits`; they are signed into the change's
provenance.

Every `rill` command accepts `--json` and prints the same encoding the MCP
tools return.

## Review policy

Labels default to `Review` (-2..+2), `Verified` (-1..+1) and `Owner`
(-1..+1, humans only at the maximum). The default Cedar submit policy:

* trivial / low risk: `Verified >= 1`, no `Review -2`
* normal: a human `Review +2`, or two agent `Review +1` plus a human `Owner +1`
* sensitive / critical: human `Owner +1` and human `Review +2`
* any unresolved Bug or Security thread blocks

Risk is computed per version from the diff (docs-only, tests-only, sensitive
path globs, size, agent-resolved conflicts) and the reasons are shown by
`rill review show`. Replace the policy with
`rill admin policy set demo/submit policy.cedar`.

## Permissions

A freshly created repo grants its creator `owner`. Add relations with

```
rill access grant team:platform owner
rill access grant kind:agent reader billing
rill access who writer billing        # everyone who may write there
```

Owner ⊇ writer ⊇ reviewer ⊇ reader; directory grants inherit downward. Capability
tokens for agents: `rill admin token issue --repo demo --verbs read,write
--paths src/ --expires 2h`. A holder can derive a narrower token offline
(`attenuate` in the SDK) but never a wider one.

## Importing from git

```
rill import git ~/src/project --repo project --storage-dir /var/lib/rill
```

Runs against the storage directory of a stopped server (or a fresh one),
maps each commit to a change version with `git_sha` in its extra fields,
and lands the first-parent history onto trunk so sequence numbers follow the
mainline. Re-running is incremental.

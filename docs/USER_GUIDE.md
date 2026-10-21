# Rill user guide

How to take a new project from an empty folder to a team (humans and agents)
landing changes on one trunk. The long form with a diagram lives in the
project's "Rill User Guide" doc; this is the same content for the repo.

## Three concepts

* **Workspace**: your private in-progress state on the server (base version,
  snapshot, undo log). One per person or agent per repo; a local checkout
  mirrors one workspace. Two people never share a folder.
* **Change**: a unit of work with a stable id (`ch_...`) and one or more
  versions. Amend, rebase and resolve create new versions of the same
  change; review state follows the id.
* **Trunk**: the single landed line, numbered `trunk@1`, `trunk@2`, ...
  Only the lander writes to it, after review policy passes.

## 1. Build and start a server

    cargo build --release --workspace
    export PATH="$PWD/target/release:$PATH"
    rill dev                             # data in ~/.rill/dev, no auth, :7470 / :7471

For a real server: `rill api init-config > rill.toml`, set `storage`,
`auth` and `dev_mode = false`, then `rill api --config rill.toml`.
Clients: `export RILL_ENDPOINT=http://server:7470 RILL_TOKEN=...`.

## 2. Create the repo and a checkout

    rill repo create app
    rill checkout app ~/app && cd ~/app
    cargo init --name app .
    printf 'target/\n' > .rillignore
    rill status
    rill new -m "initial project"
    rill submit
    rill review vote <ch> Review 2 && rill review vote <ch> Verified 1
    rill land --wait                     # Landed trunk@1

## 3. The daily loop

    # edit
    rill status; rill diff
    # snapshot
    rill new -m "handle empty input"     # new change
    rill amend                           # new version, same id
    rill describe -m "better message"
    # review
    rill submit
    rill review evaluate <ch>            # what is still required
    # land
    rill land --wait

`rill undo` reverts the last workspace operation; `rill ws ops` lists them.

## 4. Review

Labels: `Review` (-2..+2, -2 vetoes), `Verified` (-1..+1, CI), `Owner`
(-1..+1, humans only at +1). Default policy: trivial/low needs Verified +1;
normal needs a human Review +2 or two agent +1 plus a human Owner +1;
sensitive/critical need human Owner +1 and human Review +2; unresolved Bug or
Security threads block.

    rill review list --status open
    rill review show <ch>
    rill review comment <ch> -m "timeout?" --path src/main.rs --line 40
    rill review vote <ch> Review 2
    rill review threads <ch>; rill review resolve-thread <ch> <thread>

Message-only or rebase-only amends keep votes; code edits drop Review.

## 5. Working with others

Each person checks out once. The lander rebases your change onto the current
trunk head when you land; you only need `rill sync` to bring trunk into your
files. On conflict (exit 3) files get markers:

    rill resolve --list
    # edit, remove markers
    rill resolve src/main.rs             # or --all
    rill amend && rill land --wait

See others: `rill ws list`, `rill ws show <ws>`, `rill review list --owner bob`.
Hand work over: `rill ws handoff <ws> bob`.

## 6. Stacks

`rill new` on an unlanded change starts a stack.

    rill stack show
    rill stack submit
    rill stack sync
    rill stack land

## 7. Agents

    rill-mcp --endpoint http://server:7470 --token <agent-token> --repo app

27 tools: `rill_read_file`, `rill_grep`, `rill_apply_edits` (edit + create a
change in one call; model/session signed into provenance), `rill_submit`,
`rill_request_land`, `rill_land_status`, `rill_intent_*`, `rill_workspace_*`.
Narrow tokens: `rill admin token issue --for agent:fixer --repo app
--verbs read,write,review --paths src/ --expires 4h`.

## 8. History and search without a checkout

    rill --repo app log [-n 20] [--path p] [--first-parent] [ROOT]
    rill --repo app show <hash|ch_...>
    rill --repo app ls|cat <path> --at trunk@42
    rill --repo app blame <path>
    rill --repo app grep 'fn parse' -i --glob '*.rs'
    rill --repo app diff --base trunk@40 --target trunk@42
    rill --repo app trunk | landings | verify <seq> | bisect <a> <b> | revert <seq> -m ...
    rill --repo app watch trunk

`--json` on anything gives the same encoding the MCP tools return.

## 9. Team setup

    rill admin relation add repo writer team:backend
    rill admin relation add dir:/billing owner principal:<id>
    rill admin policy get app/submit > submit.cedar   # edit, then policy set
    rill admin policy get app/risk > risk.json        # add sensitive globs

Verification: `.rill/verify.toml` with `[[step]] name, command, timeout_secs`;
enable `verify` and `postland` in `rill.toml`. Import a git project:
`rill import git ~/old --repo app --storage-dir /var/lib/rill`.

## Exit codes

0 ok, 1 error, 2 usage/config, 3 conflict or not landable.

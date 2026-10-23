# Rill for VS Code

Rill source control in the editor. Open a Rill checkout (a folder with
`.rill/mount.json`) and the extension adds:

- **Source Control**: the working change's files, with conflicts first.
  Each file has a diff against the base, and the gutter shows changes.
  Commit from the message box; sync, land and refresh from the title bar.
- **Inline blame**: the author and trunk position of the line under the
  cursor. Toggle it with *Rill: Toggle Inline Blame*.
- **Conflicts**: CodeLens over each conflict region (Rill's markers, not
  git's) to take one side or all of them. *Mark resolved* runs
  `rill resolve`; *Resolve Conflicts Interactively* opens `rill resolve -i`.
- **Review threads**: the working change's threads, shown on their lines.
  You can comment on any line, reply, and resolve or reopen a thread.
- **Inbox**: changes waiting on you and your open changes, in the Source
  Control side bar.
- **Status bar**: the working change, and trunk health (green, failing or
  frozen).

The extension runs the `rill` CLI with `--json`; the CLI talks to the
server and the client daemon. Set `rill.path` if `rill` is not on your
`PATH`.

## Development

From the repository root (one pnpm workspace):

    pnpm install
    pnpm --filter rill test                                   # unit and contract tests
    RILL_BIN=$PWD/target/debug/rill pnpm --filter rill test   # plus the end-to-end test
    pnpm --filter rill package                                # rill-<version>.vsix

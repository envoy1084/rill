# Conflicts

A conflict happens when your local edits and an upstream change touch the
same lines. Rill writes both sides into the file with markers and flags
the path; nothing can be recorded until it is resolved.

    rill sync                 # merges, and reports conflicted paths
    rill conflicts            # lists them

## Resolve

- Interactively: `rill resolve -i` walks each conflict, shows the base
  and each side, and asks which to keep (a side, all sides in order, the
  base, or later).
- By hand: edit the file, remove the markers, then `rill resolve <path>`.
  Rill refuses a file that still has markers (`--force` if they are real
  content).

Merges are line-based first, then structural (they understand code), so
many edits to the same region merge cleanly on their own. Set
`RILL_STRUCTURAL_MERGE=false` to turn the structural step off.

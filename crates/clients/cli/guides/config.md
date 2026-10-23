# Configuration

`~/.config/rill/config.toml` (or `$RILL_CONFIG`) holds the server and the
CLI's own settings:

    [server]
    endpoint = "https://rill.example.com"

    [color]
    ui = "auto"               # auto | always | never (also --color, NO_COLOR)
    added = "green bold"      # restyle any role: colours, bold, dim, ul, "white on red"

    [core]
    pager = "less -FRX"       # also $RILL_PAGER, $PAGER; "cat" turns paging off

    [diff]
    syntax = true             # colour code in diffs by language
    theme = "base16-ocean.dark"

    [alias]
    st = "status"
    lg = "log --oneline"

Roles: added, removed, modified, conflict, id, ref, author, date, header,
hint, good, warning, error, dim, bold, diff-meta, diff-hunk, diff-add,
diff-del, diff-add-word, diff-del-word, graph-working, graph-change,
graph-trunk, graph-line.

# Checkouts

A checkout is served by the daemon: a base (a version on the server) and
your local edits on top, synced to a server workspace a couple of seconds
after you stop typing.

    rill checkout app work            # files on disk (eager)
    rill checkout app work --lazy     # a virtual filesystem: nothing fetched until opened
    rill checkout app ci --snapshot   # a read-only view of one version, for CI
    rill daemon status                # the daemon, its cache, its checkouts
    rill daemon unmount work          # stop serving a checkout
    rill doctor                       # how this host serves checkouts, and what to fix

Lazy checkouts use FUSE on Linux and a local NFS mount on macOS; no kernel
extension is needed. `status` answers from the daemon's record of your
edits, without walking the tree.

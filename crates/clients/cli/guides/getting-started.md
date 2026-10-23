# Getting started

Rill keeps history on the server; your machine holds checkouts, served by
a small background daemon (`rilld`) that starts on its own.

## Sign in and clone

    rill login --org acme
    rill clone app            # lazy where the platform allows; --eager writes files
    cd app

## Make a change

    $EDITOR src/main.rs
    rill status               # what differs from the base
    rill diff                 # the edits, with changed words highlighted
    rill new -m "Fix parser"  # record them as a new change (or omit -m for your editor)

Keep editing and run `rill amend` (or `rill commit`) to record the next
version of the same change.

## Review and land

    rill submit               # open the review
    rill land --wait          # queue it; the checkout follows trunk afterwards

`rill log` draws your unlanded changes above trunk; `rill inbox` lists
reviews waiting on you. See `rill help stacks`, `rill help conflicts`.

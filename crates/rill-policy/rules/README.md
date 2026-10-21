# Vendored secret-detection rules

`gitleaks.toml` is the default rule set of [gitleaks](https://github.com/gitleaks/gitleaks)
(MIT, see `LICENSE-gitleaks`), unmodified. Rill compiles it in and applies
it the way gitleaks does (keywords, secret group, entropy, allowlists).
To update: replace the file with a newer upstream release and run the
`rill-policy` tests.

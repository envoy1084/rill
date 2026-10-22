# Security policy

## Reporting a vulnerability

Please do not open a public issue for a security problem. Report it
privately through GitHub's
[private vulnerability reporting](https://github.com/rill-vcs/rill/security/advisories/new)
for this repository, or by email to vedant@envoy1084.xyz.

Include what you can of:

- the affected component (crate, RPC, CLI command or configuration);
- the commit you tested;
- steps to reproduce, or a proof of concept;
- the impact as you understand it.

You will get an acknowledgement within a few days. We will keep you informed
while the issue is investigated and fixed, and credit you in the advisory
unless you prefer otherwise. Please give us a reasonable time to ship a fix
before disclosing.

## Supported versions

Rill is pre-release and has no supported releases yet. Fixes land on `main`
only.

## Scope

Rill's threat model, the attacks it defends against and the ones it does not
yet, is documented in [threat model](architecture/05-policy/threat-model.md).
Reports of a weakness the page lists as open are still welcome, especially
with a concrete exploit.

The following are out of scope:

- denial of service by volume beyond the documented
  [input and rate limits](architecture/07-runtime/api-surface.md#input-limits);
- running a server with `dev_mode` enabled, which disables authentication by
  design;
- the test fixtures under `deploy/keycloak` and other test-only credentials.

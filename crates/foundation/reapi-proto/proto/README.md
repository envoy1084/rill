# Vendored protobuf definitions

Copied unmodified, each under its
own licence, all Apache-2.0:

| Path | Source | Version |
| --- | --- | --- |
| `build/bazel/` | [bazelbuild/remote-apis](https://github.com/bazelbuild/remote-apis) | `v2.12.0` |
| `google/` | [googleapis/googleapis](https://github.com/googleapis/googleapis) | `cec791da64be8ae9fc9d5402515a144e63f673bf` |
| `buildbarn/remoteworker/` | [buildbarn/bb-remote-execution](https://github.com/buildbarn/bb-remote-execution) `pkg/proto/remoteworker` (moved here) | `e88a90ad7415d0867f38705285b36b8481b6af77` (image tag `20261020T112007Z-e88a90a`) |

Upgrade deliberately: replace the files from the new tag, update this table,
and rerun the remote execution tests.

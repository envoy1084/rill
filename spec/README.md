# Formal specifications

TLA+ models of the parts of Rill whose correctness depends on concurrency.
Each folder holds a spec, a model configuration for TLC and a `check.sh`
that runs it (Java 11+ and `tla2tools.jar`).

| Spec | Models | Page |
| --- | --- | --- |
| [sequencer](sequencer/Sequencer.tla) | Landers racing to move trunk: speculation, re-parenting, compare-and-set | [landing](../architecture/04-workflow/landing.md) |

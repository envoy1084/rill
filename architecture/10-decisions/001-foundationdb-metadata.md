# 001 · FoundationDB is the production metadata store

Status: accepted

## Context

Every correctness argument in Rill rests on conditional multi-key
transactions: trunk moves by compare-and-swap, landings write the sequence
number, landing record and review status together, and v2 adds queues,
events and leases written alongside the state they describe. v1 runs these
on redb, a single-process embedded store, so the server cannot run on more
than one machine. The target workload (30k agents, 100k landings and 3M
versions a day) needs a horizontally scalable, strictly serializable store.

## Decision

Production deployments keep all metadata in FoundationDB. redb and the
in-memory store remain for `rill dev`, single-node installs and tests,
implemented on the same keyspace layer so the code paths are identical.

## Consequences

* Keys and values are designed for FDB's limits: keys under 10 kB, values
  under 100 kB (larger values go through a splitting layer), transactions
  under 10 MB and 5 s. Rill keeps its own budget well below these (1 MB,
  1 s per transaction).
* Hot keys are avoided by construction: logs use versionstamps, counters
  use atomic adds, and the only intentionally hot key is the trunk head,
  written by one sequencer.
* Operating Rill requires operating FDB; the deployment tooling ships the
  official operator and runbooks.

## Alternatives rejected

* **PostgreSQL:** strong transactions but vertical scaling, and hot-row
  contention on the trunk head and logs.
* **CockroachDB / Spanner:** viable, but higher write latency for the
  sequencer path and cloud lock-in for Spanner.
* **etcd:** too small for the data volume.
* **Keep redb:** single node only.

## Revisit when

FDB cannot sustain the metadata write rate of the D1 workload in the load
harness with headroom, or operating it proves impractical for adopters.

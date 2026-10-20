# 004 · Job queues, event log and leases live in FoundationDB

Status: accepted

## Context

v1 publishes events on an in-process broadcast bus and keeps the land queue
and pending reverts in memory, so a restart or a slow consumer loses work
and notifications. Writing state to one system and events to another
(a dual write) loses events whenever the second write fails.

## Decision

Durable job queues, the per-repository event log, singleton leases and quota
counters are FoundationDB subspaces. A state change, its events and the jobs
it causes are written in one transaction.

## Consequences

* No lost or phantom events; consumers resume from a versionstamp cursor.
* One stateful system to operate, back up and restore.
* Queue throughput is bounded by FDB; hot queues are sharded by subject.

## Alternatives rejected

* **Kafka / NATS JetStream:** a second source of truth, dual writes, more
  operations.

## Revisit when

Event or job volume exceeds what FDB sustains alongside metadata traffic in
the load harness.

# 014 · Objects are stored and deduplicated per organization

Status: accepted

## Context

[ADR 010](010-multi-tenancy.md) prefixes every metadata key by organization,
but objects were content-addressed in one namespace for the whole instance.
Anyone who could read any repository could fetch any object in the
deployment by its hash, and asking whether a hash exists answered for every
tenant.

## Decision

Every object is stored in its organization's namespace (the org's name), and
deduplicated only within it. `ObjectStore::namespaced` gives the same
backend over another namespace; a repository's store is always its org's.

## Consequences

* A hash learned in one organization fetches nothing in another, at any
  tier, before any path permission is checked.
* Content that two organizations both hold is stored twice.
* Deleting or exporting a tenant's objects is a prefix operation, like its
  metadata.
* Per-organization encryption keys (phase 12) attach to a namespace.

## Alternatives rejected

* **One namespace with read grants alone:** grants stop fetches, but the
  existence of a hash still leaks across tenants through uploads and
  `HasObjects`, and a single grant-check bug exposes every tenant.
* **Per-repository namespaces:** forks and moves inside an organization
  would copy all content, and organizations are the tenancy boundary.

## Revisit when

Cross-organization sharing of public content becomes a requirement; it
would need an explicit shared namespace, not a return to one store.

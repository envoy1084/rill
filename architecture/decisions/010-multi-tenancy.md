# 010 · Multi-tenancy is built into the keyspace

Status: accepted

## Context

Large companies run many organizations and business units with separate
policies, quotas and data residency; a hosted offering needs the same.
Retrofitting tenancy into a keyspace is a migration of every key.

## Decision

Every key is prefixed by organization, then repository. Quotas, policies,
identity providers and encryption keys attach at the organization.

## Consequences

* Deleting or exporting a tenant is a prefix operation.
* Single-tenant installs use one organization; there is no separate code
  path.

## Alternatives rejected

* **Single-tenant only:** cheap now, a full migration later.

## Revisit when

Never; tenancy boundaries only get finer.

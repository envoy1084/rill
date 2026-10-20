# 011 · Every mutation is idempotent; every list is paginated

Status: accepted

## Context

Tens of thousands of agents retry constantly over unreliable networks. v1
has idempotency only for some workspace operations, and several lists
return every record in one response.

## Decision

Every mutating RPC accepts a client-generated `request_id`; a retry within
24 hours returns the original response, and reuse with a different body is
an error. Every list RPC is paginated with an opaque token. Every mutable
resource carries an etag that updates may require.

## Consequences

* SDKs retry safely by default.
* The server stores idempotency records with a TTL.

## Alternatives rejected

* **Best-effort retries:** duplicate versions, votes and land requests.

## Revisit when

Never.

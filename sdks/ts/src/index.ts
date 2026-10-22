// The rill.v1 API for TypeScript: generated Connect clients and messages
// (`gen/`), one client holding them all, names, error reasons and retries.

export { createRillClient, type Options, type RillClient } from "./client.js";
export * as names from "./names.js";
export { ERROR_DOMAIN, isReason, missing, reason, retryAfterMs } from "./errors.js";
export { RETRY_BUDGET_MS, retry } from "./retry.js";

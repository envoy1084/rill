// Retrying calls that may safely run again.

import { Code, ConnectError } from "@connectrpc/connect";

import { retryAfterMs } from "./errors.js";

/** How long `retry` keeps trying: past a dead server's idempotency claim. */
export const RETRY_BUDGET_MS = 20_000;

/**
 * Run `call` until it succeeds, fails for good, or the budget runs out,
 * retrying while the cluster is unavailable and while the server refuses
 * for now with a wait (a rate limit, a full queue). Safe for reads and for
 * mutations carrying a `requestId`, which the server runs at most once.
 */
export async function retry<T>(call: () => Promise<T>): Promise<T> {
  const deadline = Date.now() + RETRY_BUDGET_MS;
  let delay = 100;
  for (;;) {
    try {
      return await call();
    } catch (err) {
      const e = ConnectError.from(err);
      let wait: number | undefined;
      if (e.code === Code.Unavailable) {
        wait = delay / 2 + Math.random() * (delay / 2);
        delay = Math.min(delay * 2, 2_000);
      } else if (e.code === Code.ResourceExhausted) {
        wait = retryAfterMs(e);
      }
      if (wait === undefined) throw err;
      wait = Math.min(wait, 5_000);
      if (Date.now() + wait > deadline) throw err;
      await new Promise((resolve) => setTimeout(resolve, wait));
    }
  }
}

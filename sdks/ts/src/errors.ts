// What an error's details say: the reason, what a policy denial misses,
// and how long to wait.

import { ConnectError } from "@connectrpc/connect";

import {
  ErrorInfoSchema,
  PreconditionFailureSchema,
  RetryInfoSchema,
} from "./gen/google/rpc/error_details_pb.js";

/** The ErrorInfo domain of Rill's error reasons. */
export const ERROR_DOMAIN = "rill.dev";

/** The error's reason (`NOT_FOUND`, `ETAG_MISMATCH`, …), or undefined. */
export function reason(err: unknown): string | undefined {
  const e = ConnectError.from(err);
  return e.findDetails(ErrorInfoSchema).find((i) => i.domain === ERROR_DOMAIN)?.reason;
}

/** Whether `err` is a refusal for `why`. */
export function isReason(err: unknown, why: string): boolean {
  return reason(err) === why;
}

/** What a policy denial still needs. */
export function missing(err: unknown): string[] {
  return ConnectError.from(err)
    .findDetails(PreconditionFailureSchema)
    .flatMap((f) => f.violations.map((v) => v.description));
}

/** How long the server asked the caller to wait, in milliseconds. */
export function retryAfterMs(err: unknown): number | undefined {
  const delay = ConnectError.from(err).findDetails(RetryInfoSchema)[0]?.retryDelay;
  return delay ? Number(delay.seconds) * 1000 + Math.floor(delay.nanos / 1e6) : undefined;
}

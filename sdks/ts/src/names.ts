// Resource names, built from what callers hold.

import { randomUUID } from "node:crypto";

/** The org a bare repository name is in. */
export const DEFAULT_ORG = "default";

const hex = (bytes: Uint8Array): string => Buffer.from(bytes).toString("hex");

/** `orgs/{org}/repos/{repo}` for a bare name, `org/name` or a full name. */
export function repo(spec: string): string {
  if (spec.startsWith("orgs/")) return spec;
  const slash = spec.indexOf("/");
  return slash < 0
    ? `orgs/${DEFAULT_ORG}/repos/${spec}`
    : `orgs/${spec.slice(0, slash)}/repos/${spec.slice(slash + 1)}`;
}

/** `orgs/{org}`; the default org when `name` is empty. */
export function org(name = ""): string {
  return `orgs/${name || DEFAULT_ORG}`;
}

/** `{repo}/changes/ch_{hex}` for a change id as messages carry it. */
export function change(repoSpec: string, id: Uint8Array): string {
  return `${repo(repoSpec)}/changes/ch_${hex(id)}`;
}

/** The review of a change. */
export function review(repoSpec: string, id: Uint8Array): string {
  return `${change(repoSpec, id)}/review`;
}

/** `{repo}/versions/{hex}` for a version hash. */
export function version(repoSpec: string, hash: Uint8Array): string {
  return `${repo(repoSpec)}/versions/${hex(hash)}`;
}

/** A version hash as a revision. */
export function revision(hash: Uint8Array): string {
  return hex(hash);
}

/** `{repo}/trunk/positions/{n}`, or `head`. */
export function trunkPosition(repoSpec: string, n: bigint | "head"): string {
  return `${repo(repoSpec)}/trunk/positions/${n}`;
}

/** `{repo}/policies/{name}`. */
export function policy(repoSpec: string, name: string): string {
  return `${repo(repoSpec)}/policies/${name}`;
}

/** A fresh idempotency key. */
export function requestId(): string {
  return randomUUID();
}

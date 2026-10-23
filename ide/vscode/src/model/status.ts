// The working change's files, split into conflicts and the rest.

import type { ChangeKind, Status } from "../cli/schema";

export interface FileEntry {
  path: string;
  kind: ChangeKind;
  letter: string;
}

const LETTERS: Record<ChangeKind, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  conflict: "C",
};

export function fileGroups(status: Status): { changes: FileEntry[]; conflicts: FileEntry[] } {
  const entries = status.changes.map((c) => ({ path: c.path, kind: c.kind, letter: LETTERS[c.kind] ?? "?" }));
  return {
    changes: entries.filter((e) => e.kind !== "conflict"),
    conflicts: entries.filter((e) => e.kind === "conflict"),
  };
}

/** The root the checkout's files are compared against. */
export function baseRoot(status: Status): string {
  return status.base_version ?? status.base;
}

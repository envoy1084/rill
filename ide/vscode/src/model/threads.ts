// Review threads placed in files.

import type { Threads } from "../cli/schema";

export interface PlacedThread {
  id: string;
  path: string;
  /** 0-based, inclusive. */
  startLine: number;
  endLine: number;
  resolved: boolean;
  comments: { author: string; body: string }[];
}

/** Threads anchored to a file or lines; change-level threads have no place. */
export function placedThreads(t: Threads): PlacedThread[] {
  const out: PlacedThread[] = [];
  for (const thread of t.threads) {
    const a = thread.anchor;
    if (a.kind === "change") {
      continue;
    }
    const [start, end] = a.kind === "lines" ? [a.start - 1, a.end - 1] : [0, 0];
    out.push({
      id: thread.id,
      path: a.path,
      startLine: Math.max(0, start),
      endLine: Math.max(0, start, end),
      resolved: thread.resolved,
      comments: thread.comments.map((c) => ({ author: c.author.display, body: c.body })),
    });
  }
  return out;
}

/** Open threads anchored to the change itself. */
export function openChangeThreads(t: Threads): number {
  return t.threads.filter((x) => x.anchor.kind === "change" && !x.resolved).length;
}

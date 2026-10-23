// The inbox as tree items: what waits on you, and your open changes.

import type { Inbox, InboxEntry } from "../cli/schema";

export interface InboxItem {
  change: string;
  label: string;
  description: string;
}

export interface InboxSections {
  waiting: InboxItem[];
  yours: InboxItem[];
}

export function inboxSections(inbox: Inbox): InboxSections {
  return {
    waiting: inbox.waiting.map((e) => item(e, true)),
    yours: inbox.yours.map((e) => item(e, false)),
  };
}

function item(e: InboxEntry, showOwner: boolean): InboxItem {
  const parts = [
    e.status,
    plural(e.approvals, "approval"),
    plural(e.open_threads, "open thread"),
  ];
  if (showOwner) {
    parts.push(e.owner);
  }
  return { change: e.change, label: e.change, description: parts.join(" · ") };
}

function plural(n: number, noun: string): string {
  return `${n} ${noun}${n === 1 ? "" : "s"}`;
}

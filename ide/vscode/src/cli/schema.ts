// The `rill --json` documents the extension reads. Field names follow
// the CLI's `rill.cli.v1.*` schemas.

export interface Principal {
  id: string;
  kind: string;
  name: string;
  display: string;
}

export interface Timestamp {
  rfc3339: string;
  millis: number;
}

export type ChangeKind = "added" | "modified" | "deleted" | "conflict";

export interface Status {
  repo: string;
  base: string;
  base_version: string | null;
  working_change: string | null;
  workspace: string | null;
  pending_sync: number;
  changes: { path: string; kind: ChangeKind }[];
  conflicts: number;
}

export interface BlameLine {
  line_no: number;
  version: string;
  author: Principal;
  trunk_seq: number | null;
  text: string;
}

export interface Blame {
  path: string;
  version: string;
  lines: BlameLine[];
}

export interface Health {
  state: string;
  since: number;
  open: string[];
  failing_steps: string[];
  frozen: boolean;
  frozen_reason: string | null;
}

export interface TrunkHealth {
  health: Health;
}

export interface InboxEntry {
  change: string;
  owner: string;
  status: string;
  approvals: number;
  open_threads: number;
}

export interface Inbox {
  waiting: InboxEntry[];
  yours: InboxEntry[];
}

export type Anchor =
  | { kind: "change" }
  | { kind: "file"; version: string; path: string }
  | { kind: "lines"; version: string; path: string; start: number; end: number };

export interface Comment {
  author: Principal;
  body: string;
  created_at: Timestamp;
}

export interface Thread {
  id: string;
  anchor: Anchor;
  comments: Comment[];
  resolved: boolean;
}

export interface Threads {
  change_id: string;
  threads: Thread[];
}

export interface Land {
  status: string;
  seq: number | null;
  error: string | null;
  position: number | null;
}

export interface Conflicts {
  conflicts: string[];
}

export interface FileBytes {
  encoding: "utf8" | "hex";
  size: number;
  text?: string;
  hex?: string;
}

export interface CliErrorBody {
  code: string;
  reason: string | null;
  message: string;
  missing: string[];
  exit: number;
}

// The commands the extension runs, typed.

import { run, type Runner } from "./run";
import type {
  Blame,
  Conflicts,
  FileBytes,
  Inbox,
  Land,
  Status,
  Thread,
  Threads,
  TrunkHealth,
} from "./schema";

/** One checkout's `rill`. */
export class Rill {
  constructor(readonly runner: Runner) {}

  status(): Promise<Status> {
    return run(this.runner, ["status"], "status");
  }

  /** Blame `path` (a repository path) at a version, default trunk. */
  blame(path: string, at?: string): Promise<Blame> {
    return run(this.runner, ["blame", path, ...(at ? ["--at", at] : [])], "blame");
  }

  /** A file's bytes at a version. */
  cat(path: string, at: string): Promise<FileBytes> {
    return run(this.runner, ["cat", path, "--at", at], "cat");
  }

  trunkHealth(): Promise<TrunkHealth> {
    return run(this.runner, ["trunk", "health"], "trunk.health");
  }

  inbox(): Promise<Inbox> {
    return run(this.runner, ["inbox"], "inbox");
  }

  threads(change?: string): Promise<Threads> {
    return run(this.runner, ["review", "threads", ...(change ? [change] : [])], "review.threads");
  }

  comment(change: string, body: string, path?: string, line?: number): Promise<{ thread_id: string }> {
    const anchor = path ? ["--path", path, ...(line ? ["--line", String(line)] : [])] : [];
    return run(this.runner, ["review", "comment", change, "-m", body, ...anchor], "review.comment");
  }

  reply(change: string, thread: string, body: string): Promise<Thread> {
    return run(this.runner, ["review", "reply", change, thread, "-m", body], "review.reply");
  }

  resolveThread(change: string, thread: string, reopen: boolean): Promise<Thread> {
    const flag = reopen ? ["--reopen"] : [];
    return run(this.runner, ["review", "resolve-thread", change, thread, ...flag], "review.resolve_thread");
  }

  conflicts(): Promise<Conflicts> {
    return run(this.runner, ["conflicts"], "conflicts");
  }

  resolve(paths: string[]): Promise<unknown> {
    return run(this.runner, ["resolve", ...paths], "resolve");
  }

  describe(message: string): Promise<unknown> {
    return run(this.runner, ["describe", "-m", message], "describe");
  }

  commit(message: string): Promise<unknown> {
    return run(this.runner, ["commit", "-m", message], "commit");
  }

  submit(): Promise<unknown> {
    return run(this.runner, ["submit"], "submit");
  }

  sync(): Promise<unknown> {
    return run(this.runner, ["sync"], "sync");
  }

  land(timeoutSeconds: number): Promise<Land> {
    return run(this.runner, ["land", "--wait", "--timeout", String(timeoutSeconds)], "land");
  }
}

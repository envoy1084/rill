// One Rill checkout open in the editor: its `rill`, its latest status,
// and change events the other features follow.

import * as path from "node:path";
import * as vscode from "vscode";

import { Rill } from "../cli/rill";
import { RillError } from "../cli/run";
import type { Status } from "../cli/schema";

export class Checkout implements vscode.Disposable {
  readonly rill: Rill;
  status: Status | undefined;
  /** Its Source Control entry, once registered. */
  scm: vscode.SourceControl | undefined;
  private readonly changed = new vscode.EventEmitter<Status>();
  readonly onDidChange = this.changed.event;
  private running: Promise<void> | undefined;
  private again = false;

  constructor(
    readonly root: vscode.Uri,
    binary: string,
    readonly log: vscode.OutputChannel,
  ) {
    this.rill = new Rill({ binary, cwd: root.fsPath });
  }

  /** Re-read status; overlapping calls coalesce into one more run. */
  refresh(): Promise<void> {
    if (this.running) {
      this.again = true;
      return this.running;
    }
    this.running = (async () => {
      do {
        this.again = false;
        try {
          this.status = await this.rill.status();
          this.changed.fire(this.status);
        } catch (e) {
          this.report("status", e);
        }
      } while (this.again);
      this.running = undefined;
    })();
    return this.running;
  }

  /** The repository path of `uri`, if it is inside this checkout. */
  repoPath(uri: vscode.Uri): string | undefined {
    if (uri.scheme !== "file") {
      return undefined;
    }
    const rel = path.relative(this.root.fsPath, uri.fsPath);
    if (!rel || rel.startsWith("..") || path.isAbsolute(rel) || rel.split(path.sep)[0] === ".rill") {
      return undefined;
    }
    return rel.split(path.sep).join("/");
  }

  /** The file for a repository path. */
  uri(repoPath: string): vscode.Uri {
    return vscode.Uri.joinPath(this.root, ...repoPath.split("/"));
  }

  /** Log a background failure. */
  report(what: string, e: unknown): void {
    const msg = e instanceof RillError ? `${e.code}: ${e.message}` : String(e);
    this.log.appendLine(`[${new Date().toISOString()}] ${what}: ${msg}`);
  }

  dispose(): void {
    this.changed.dispose();
  }
}

/** Run a user action, showing its failure. */
export async function act<T>(what: string, f: () => Promise<T>): Promise<T | undefined> {
  try {
    return await f();
  } catch (e) {
    const msg = e instanceof RillError ? e.message : String(e);
    const missing = e instanceof RillError && e.missing.length ? ` (needs: ${e.missing.join(", ")})` : "";
    void vscode.window.showErrorMessage(`Rill ${what} failed: ${msg}${missing}`);
    return undefined;
  }
}

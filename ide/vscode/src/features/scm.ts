// The Source Control view: the working change's files, conflicts first,
// with diffs against the base and gutter changes (quick diff).

import * as vscode from "vscode";

import type { FileBytes } from "../cli/schema";
import { baseRoot, fileGroups, type FileEntry } from "../model/status";
import type { Checkout } from "./checkout";

export const BASE_SCHEME = "rill-base";

export function registerScm(checkout: Checkout): vscode.Disposable[] {
  const scm = vscode.scm.createSourceControl("rill", "Rill", checkout.root);
  checkout.scm = scm;
  scm.inputBox.placeholder = "Message (⌘Enter to commit the working change)";
  scm.acceptInputCommand = { command: "rill.commit", title: "Commit" };
  const conflicts = scm.createResourceGroup("conflicts", "Conflicts");
  conflicts.hideWhenEmpty = true;
  const changes = scm.createResourceGroup("changes", "Working change");
  scm.quickDiffProvider = {
    provideOriginalResource(uri) {
      const p = checkout.repoPath(uri);
      return p && checkout.status ? baseUri(p, baseRoot(checkout.status)) : undefined;
    },
  };
  const update = checkout.onDidChange((status) => {
    const groups = fileGroups(status);
    const root = baseRoot(status);
    conflicts.resourceStates = groups.conflicts.map((e) => state(checkout, e, root));
    changes.resourceStates = groups.changes.map((e) => state(checkout, e, root));
    scm.count = status.changes.length;
    changes.label = status.working_change ? `Working change ${status.working_change.slice(0, 11)}` : "Working change";
  });
  return [scm, update];
}

function state(checkout: Checkout, e: FileEntry, root: string): vscode.SourceControlResourceState {
  const file = checkout.uri(e.path);
  const base = baseUri(e.path, root);
  const title = `${e.path} (${root.slice(0, 12)} ↔ working)`;
  const command: vscode.Command =
    e.kind === "added" || e.kind === "conflict"
      ? { command: "vscode.open", title: "Open", arguments: [file] }
      : e.kind === "deleted"
        ? { command: "vscode.open", title: "Open", arguments: [base] }
        : { command: "vscode.diff", title: "Diff", arguments: [base, file, title] };
  return {
    resourceUri: file,
    command,
    decorations: {
      strikeThrough: e.kind === "deleted",
      tooltip: `${e.kind} (${e.letter})`,
    },
  };
}

/** A read-only view of `path` at `root`. */
export function baseUri(path: string, root: string): vscode.Uri {
  return vscode.Uri.from({ scheme: BASE_SCHEME, path: `/${path}`, query: root });
}

/** Serves `rill-base:` documents with `rill cat`. */
export function baseContent(checkout: () => Checkout | undefined): vscode.TextDocumentContentProvider {
  return {
    async provideTextDocumentContent(uri) {
      const c = checkout();
      if (!c) {
        return "";
      }
      try {
        const bytes: FileBytes = await c.rill.cat(uri.path.replace(/^\//, ""), uri.query);
        return bytes.encoding === "utf8" ? (bytes.text ?? "") : `(binary, ${bytes.size} bytes)`;
      } catch (e) {
        c.report("cat", e);
        return "";
      }
    },
  };
}

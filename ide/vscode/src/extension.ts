// Activation: find Rill checkouts among the workspace folders and wire
// every feature to them. The extension drives the `rill` CLI's JSON
// output, which talks to the server and the client daemon.

import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";

import { registerActions } from "./features/actions";
import { registerBlame } from "./features/blame";
import { Checkout } from "./features/checkout";
import { registerConflicts } from "./features/conflicts";
import { InboxView } from "./features/inbox";
import { registerReview } from "./features/review";
import { BASE_SCHEME, baseContent, registerScm } from "./features/scm";
import { registerStatusBar } from "./features/statusBar";

export function activate(context: vscode.ExtensionContext): void {
  const config = () => vscode.workspace.getConfiguration("rill");
  const binary = () => config().get<string>("path", "rill");
  const log = vscode.window.createOutputChannel("Rill");
  context.subscriptions.push(log);

  const checkouts = (vscode.workspace.workspaceFolders ?? [])
    .filter((f) => fs.existsSync(path.join(f.uri.fsPath, ".rill", "mount.json")))
    .map((f) => new Checkout(f.uri, binary(), log));
  if (checkouts.length === 0) {
    return;
  }
  void vscode.commands.executeCommand("setContext", "rill.active", true);
  const current = (): Checkout | undefined => {
    const uri = vscode.window.activeTextEditor?.document.uri;
    return (uri && checkouts.find((c) => c.repoPath(uri) !== undefined)) ?? checkouts[0];
  };

  context.subscriptions.push(
    ...checkouts,
    vscode.workspace.registerTextDocumentContentProvider(BASE_SCHEME, baseContent(current)),
    ...registerActions(current, binary),
    ...registerConflicts(current),
  );

  const inbox = new InboxView(checkouts[0]);
  context.subscriptions.push(vscode.window.registerTreeDataProvider("rill.inbox", inbox));
  const polls: (() => Promise<void>)[] = [() => inbox.poll()];

  for (const checkout of checkouts) {
    const status = registerStatusBar(checkout);
    const review = registerReview(checkout);
    polls.push(status.poll, review.poll);
    context.subscriptions.push(...registerScm(checkout), ...registerBlame(checkout), ...status.items, ...review.items);
    context.subscriptions.push(checkout.onDidChange(() => void review.poll()));

    let timer: NodeJS.Timeout | undefined;
    const watcher = vscode.workspace.createFileSystemWatcher(new vscode.RelativePattern(checkout.root, "**/*"));
    const changed = (uri: vscode.Uri) => {
      if (checkout.repoPath(uri) !== undefined) {
        clearTimeout(timer);
        timer = setTimeout(() => void checkout.refresh(), 500);
      }
    };
    context.subscriptions.push(
      watcher,
      watcher.onDidChange(changed),
      watcher.onDidCreate(changed),
      watcher.onDidDelete(changed),
      { dispose: () => clearTimeout(timer) },
    );
    void checkout.refresh();
  }

  const pollAll = () => Promise.all(polls.map((p) => p())).then(() => undefined);
  void pollAll();
  const seconds = Math.max(5, config().get<number>("refreshSeconds", 30));
  const interval = setInterval(() => void pollAll(), seconds * 1000);
  context.subscriptions.push({ dispose: () => clearInterval(interval) });
  context.subscriptions.push(
    vscode.commands.registerCommand("rill.refresh", async () => {
      await Promise.all(checkouts.map((c) => c.refresh()));
      await pollAll();
    }),
  );
}

export function deactivate(): void {}

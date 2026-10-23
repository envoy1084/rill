// Commands that move the working change along: describe, commit, submit,
// sync, resolve, land, and the full-screen review in a terminal.

import * as vscode from "vscode";

import { act, type Checkout } from "./checkout";

export function registerActions(checkout: () => Checkout | undefined, binary: () => string): vscode.Disposable[] {
  const withCheckout =
    <A extends unknown[]>(f: (c: Checkout, ...args: A) => Promise<unknown>) =>
    async (...args: A) => {
      const c = checkout();
      if (!c) {
        void vscode.window.showWarningMessage("No Rill checkout is open.");
        return;
      }
      await f(c, ...args);
      await c.refresh();
    };

  const message = async (scm: vscode.SourceControl | undefined, prompt: string): Promise<string | undefined> => {
    const typed = scm?.inputBox.value.trim();
    if (typed) {
      return typed;
    }
    return vscode.window.showInputBox({
      prompt,
      ignoreFocusOut: true,
      validateInput: (v) => (v.trim() ? null : "A message is required"),
    });
  };

  const terminal = (c: Checkout, args: string) => {
    const t = vscode.window.createTerminal({ name: "rill", cwd: c.root });
    t.show();
    t.sendText(`${JSON.stringify(binary())} ${args}`);
  };

  return [
    vscode.commands.registerCommand(
      "rill.commit",
      withCheckout(async (c) => {
        const scm = c.scm;
        const msg = await message(scm, "Commit message");
        if (msg && (await act("commit", () => c.rill.commit(msg))) && scm) {
          scm.inputBox.value = "";
        }
      }),
    ),
    vscode.commands.registerCommand(
      "rill.describe",
      withCheckout(async (c) => {
        const msg = await message(c.scm, "Describe the working change");
        if (msg) {
          await act("describe", () => c.rill.describe(msg));
        }
      }),
    ),
    vscode.commands.registerCommand(
      "rill.submit",
      withCheckout(async (c) => {
        if (await act("submit", () => c.rill.submit())) {
          void vscode.window.showInformationMessage("Submitted for review.");
        }
      }),
    ),
    vscode.commands.registerCommand("rill.sync", withCheckout((c) => act("sync", () => c.rill.sync()))),
    vscode.commands.registerCommand(
      "rill.markResolved",
      withCheckout(async (c, target?: vscode.Uri | vscode.SourceControlResourceState) => {
        const uri = target instanceof vscode.Uri ? target : target?.resourceUri;
        const path = uri && c.repoPath(uri);
        if (path) {
          await act("resolve", () => c.rill.resolve([path]));
        }
      }),
    ),
    vscode.commands.registerCommand("rill.resolveInteractive", withCheckout(async (c) => terminal(c, "resolve -i"))),
    vscode.commands.registerCommand(
      "rill.reviewOpen",
      withCheckout(async (c, change?: string) => terminal(c, `review open${typeof change === "string" ? ` ${change}` : ""}`)),
    ),
    vscode.commands.registerCommand(
      "rill.land",
      withCheckout(async (c) => {
        const land = await vscode.window.withProgress(
          { location: vscode.ProgressLocation.Notification, title: "Landing the working change…" },
          () => act("land", () => c.rill.land(300)),
        );
        if (!land) {
          return;
        }
        if (land.status === "landed") {
          void vscode.window.showInformationMessage(`Landed as trunk@${land.seq}.`);
        } else if (land.status === "failed" || land.status === "cancelled") {
          const next = await vscode.window.showErrorMessage(`Landing ${land.status}: ${land.error ?? ""}`, "Sync onto trunk");
          if (next) {
            await vscode.commands.executeCommand("rill.sync");
          }
        } else {
          void vscode.window.showInformationMessage(`Still landing (${land.status}); it continues on the server.`);
        }
      }),
    ),
  ];
}

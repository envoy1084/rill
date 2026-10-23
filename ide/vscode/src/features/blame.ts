// Inline blame: who last changed the current line, after its end.

import * as vscode from "vscode";

import type { Blame } from "../cli/schema";
import { annotation, blameFor } from "../model/blame";
import { baseRoot } from "../model/status";
import type { Checkout } from "./checkout";

export function registerBlame(checkout: Checkout): vscode.Disposable[] {
  const decoration = vscode.window.createTextEditorDecorationType({
    after: { margin: "0 0 0 3em", color: new vscode.ThemeColor("editorCodeLens.foreground") },
    rangeBehavior: vscode.DecorationRangeBehavior.ClosedOpen,
  });
  const cache = new Map<string, Promise<Blame | undefined>>();
  let timer: NodeJS.Timeout | undefined;

  const enabled = () => vscode.workspace.getConfiguration("rill").get<boolean>("blame.enabled", true);

  const show = async (editor: vscode.TextEditor | undefined) => {
    if (!editor) {
      return;
    }
    const p = checkout.repoPath(editor.document.uri);
    if (!p || !checkout.status || !enabled()) {
      editor.setDecorations(decoration, []);
      return;
    }
    const at = baseRoot(checkout.status);
    const key = `${at}\0${p}`;
    if (!cache.has(key)) {
      cache.set(
        key,
        checkout.rill.blame(p, at).catch((e) => {
          checkout.report("blame", e);
          return undefined;
        }),
      );
    }
    const blame = await cache.get(key);
    const line = editor.selection.active.line;
    if (!blame || editor !== vscode.window.activeTextEditor || line >= editor.document.lineCount) {
      return;
    }
    const blamed = blameFor(blame, line + 1, editor.document.lineAt(line).text);
    const end = editor.document.lineAt(line).range.end;
    editor.setDecorations(
      decoration,
      blamed ? [{ range: new vscode.Range(end, end), renderOptions: { after: { contentText: annotation(blamed) } } }] : [],
    );
  };

  const soon = (editor: vscode.TextEditor | undefined) => {
    clearTimeout(timer);
    timer = setTimeout(() => void show(editor), 150);
  };

  return [
    decoration,
    vscode.window.onDidChangeTextEditorSelection((e) => soon(e.textEditor)),
    vscode.window.onDidChangeActiveTextEditor(soon),
    checkout.onDidChange(() => {
      cache.clear();
      soon(vscode.window.activeTextEditor);
    }),
    vscode.commands.registerCommand("rill.toggleBlame", async () => {
      await vscode.workspace.getConfiguration("rill").update("blame.enabled", !enabled(), true);
      soon(vscode.window.activeTextEditor);
    }),
    { dispose: () => clearTimeout(timer) },
  ];
}

// Conflicts in the editor: CodeLens over each conflict region to take a
// side (or all of them), since Rill's markers are not git's.

import * as vscode from "vscode";

import { parseConflicts } from "../model/markers";
import type { Checkout } from "./checkout";

export function registerConflicts(checkout: () => Checkout | undefined): vscode.Disposable[] {
  const lenses: vscode.CodeLensProvider = {
    provideCodeLenses(doc) {
      if (!checkout()?.repoPath(doc.uri)) {
        return [];
      }
      return parseConflicts(doc.getText()).flatMap((region, index) => {
        const at = new vscode.Range(region.startLine, 0, region.startLine, 0);
        const sides = region.sides.map(
          (_, side) =>
            new vscode.CodeLens(at, {
              title: `Take side #${side + 1}`,
              command: "rill.takeSide",
              arguments: [doc.uri, index, side],
            }),
        );
        const all = new vscode.CodeLens(at, {
          title: "Take all sides",
          command: "rill.takeSide",
          arguments: [doc.uri, index, -1],
        });
        return [...sides, all];
      });
    },
  };

  const take = async (uri: vscode.Uri, index: number, side: number) => {
    const doc = await vscode.workspace.openTextDocument(uri);
    const region = parseConflicts(doc.getText())[index];
    if (!region) {
      return;
    }
    const text = side < 0 ? region.sides.join("") : (region.sides[side] ?? "");
    const edit = new vscode.WorkspaceEdit();
    edit.replace(uri, new vscode.Range(region.startLine, 0, region.endLine + 1, 0), text);
    await vscode.workspace.applyEdit(edit);
    if (parseConflicts(doc.getText()).length === 0) {
      const path = checkout()?.repoPath(uri);
      const choice = await vscode.window.showInformationMessage(
        `No conflicts left in ${path}. Save it and mark it resolved?`,
        "Mark resolved",
      );
      if (choice) {
        await doc.save();
        await vscode.commands.executeCommand("rill.markResolved", uri);
      }
    }
  };

  return [
    vscode.languages.registerCodeLensProvider({ scheme: "file" }, lenses),
    vscode.commands.registerCommand("rill.takeSide", take),
  ];
}

// Status bar: the working change and trunk health.

import * as vscode from "vscode";

import { healthView } from "../model/health";
import type { Checkout } from "./checkout";

export function registerStatusBar(checkout: Checkout): { items: vscode.Disposable[]; poll: () => Promise<void> } {
  const change = vscode.window.createStatusBarItem("rill.change", vscode.StatusBarAlignment.Left, 100);
  change.name = "Rill working change";
  change.command = "rill.reviewOpen";
  const health = vscode.window.createStatusBarItem("rill.health", vscode.StatusBarAlignment.Left, 99);
  health.name = "Rill trunk health";

  const sub = checkout.onDidChange((s) => {
    const id = s.working_change ? s.working_change.slice(0, 11) : "no change";
    change.text = `$(git-commit) ${id}`;
    const conflicts = s.conflicts ? `, ${s.conflicts} conflicted` : "";
    change.tooltip = `${s.repo} on ${s.base}: ${s.changes.length} file(s) changed${conflicts}`;
    change.show();
  });

  const poll = async () => {
    try {
      const view = healthView((await checkout.rill.trunkHealth()).health);
      health.text = view.text;
      health.tooltip = view.tooltip;
      health.backgroundColor = view.alert ? new vscode.ThemeColor("statusBarItem.warningBackground") : undefined;
      health.show();
    } catch (e) {
      checkout.report("trunk health", e);
      health.hide();
    }
  };
  return { items: [change, health, sub], poll };
}

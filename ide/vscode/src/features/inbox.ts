// The inbox view: changes waiting on you, and your open changes.

import * as vscode from "vscode";

import { inboxSections, type InboxItem, type InboxSections } from "../model/inbox";
import type { Checkout } from "./checkout";

type Node = { section: keyof InboxSections; label: string } | InboxItem;

export class InboxView implements vscode.TreeDataProvider<Node> {
  private sections: InboxSections = { waiting: [], yours: [] };
  private readonly changed = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.changed.event;

  constructor(private readonly checkout: Checkout) {}

  async poll(): Promise<void> {
    try {
      this.sections = inboxSections(await this.checkout.rill.inbox());
      this.changed.fire();
    } catch (e) {
      this.checkout.report("inbox", e);
    }
  }

  getChildren(node?: Node): Node[] {
    if (!node) {
      return [
        { section: "waiting", label: `Waiting on you (${this.sections.waiting.length})` },
        { section: "yours", label: `Your changes (${this.sections.yours.length})` },
      ];
    }
    return "section" in node ? this.sections[node.section] : [];
  }

  getTreeItem(node: Node): vscode.TreeItem {
    if ("section" in node) {
      return new vscode.TreeItem(node.label, vscode.TreeItemCollapsibleState.Expanded);
    }
    const item = new vscode.TreeItem(node.label);
    item.description = node.description;
    item.iconPath = new vscode.ThemeIcon("git-pull-request");
    item.command = { command: "rill.reviewOpen", title: "Open review", arguments: [node.change] };
    return item;
  }
}

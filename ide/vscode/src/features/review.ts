// Review threads of the working change, in the editor: read, comment on
// lines, reply, resolve and reopen.

import * as vscode from "vscode";

import { placedThreads } from "../model/threads";
import { act, type Checkout } from "./checkout";

const IDS = new WeakMap<vscode.CommentThread, string>();

export function registerReview(checkout: Checkout): { items: vscode.Disposable[]; poll: () => Promise<void> } {
  const controller = vscode.comments.createCommentController("rill", "Rill review");
  controller.options = { prompt: "Comment on this change", placeHolder: "Write a review comment" };
  let shown: vscode.CommentThread[] = [];
  controller.commentingRangeProvider = {
    provideCommentingRanges(doc) {
      return checkout.repoPath(doc.uri) && checkout.status?.working_change
        ? [new vscode.Range(0, 0, Math.max(0, doc.lineCount - 1), 0)]
        : [];
    },
  };

  const poll = async () => {
    const change = checkout.status?.working_change;
    let placed: ReturnType<typeof placedThreads> = [];
    if (change) {
      try {
        placed = placedThreads(await checkout.rill.threads(change));
      } catch (e) {
        checkout.report("review threads", e);
      }
    }
    shown.forEach((t) => t.dispose());
    shown = placed.map((p) => {
      const thread = controller.createCommentThread(
        checkout.uri(p.path),
        new vscode.Range(p.startLine, 0, p.endLine, 0),
        p.comments.map((c) => ({
          body: new vscode.MarkdownString(c.body),
          mode: vscode.CommentMode.Preview,
          author: { name: c.author },
        })),
      );
      IDS.set(thread, p.id);
      thread.label = p.resolved ? "Resolved" : "Open";
      thread.contextValue = p.resolved ? "resolved" : "open";
      thread.state = p.resolved ? vscode.CommentThreadState.Resolved : vscode.CommentThreadState.Unresolved;
      thread.collapsibleState = p.resolved
        ? vscode.CommentThreadCollapsibleState.Collapsed
        : vscode.CommentThreadCollapsibleState.Expanded;
      return thread;
    });
  };

  const reply = async (r: vscode.CommentReply) => {
    const change = checkout.status?.working_change;
    const path = checkout.repoPath(r.thread.uri);
    if (!change || !path) {
      return;
    }
    const id = IDS.get(r.thread);
    await act("comment", async () => {
      if (id) {
        await checkout.rill.reply(change, id, r.text);
      } else {
        await checkout.rill.comment(change, r.text, path, r.thread.range ? r.thread.range.start.line + 1 : undefined);
        r.thread.dispose();
      }
      await poll();
    });
  };

  const setResolved = (reopen: boolean) => async (thread: vscode.CommentThread) => {
    const change = checkout.status?.working_change;
    const id = IDS.get(thread);
    if (change && id) {
      await act(reopen ? "reopen" : "resolve", async () => {
        await checkout.rill.resolveThread(change, id, reopen);
        await poll();
      });
    }
  };

  return {
    items: [
      controller,
      { dispose: () => shown.forEach((t) => t.dispose()) },
      vscode.commands.registerCommand("rill.reply", reply),
      vscode.commands.registerCommand("rill.resolveThread", setResolved(false)),
      vscode.commands.registerCommand("rill.reopenThread", setResolved(true)),
    ],
    poll,
  };
}

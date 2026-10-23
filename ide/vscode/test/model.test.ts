import { strict as assert } from "node:assert";
import { test } from "node:test";

import type { Blame, Threads } from "../src/cli/schema";
import { annotation, blameFor } from "../src/model/blame";
import { healthView } from "../src/model/health";
import { inboxSections } from "../src/model/inbox";
import { openChangeThreads, placedThreads } from "../src/model/threads";

const dev = { id: "pr_1", kind: "human", name: "dev", display: "dev" };

test("blame annotates only lines that still read as blamed", () => {
  const blame: Blame = {
    path: "a.rs",
    version: "v",
    lines: [
      { line_no: 1, version: "aaaaaaaaaaaaaaaa", author: dev, trunk_seq: 7, text: "one\n" },
      { line_no: 2, version: "bbbbbbbbbbbbbbbb", author: dev, trunk_seq: null, text: "two\r\n" },
    ],
  };
  assert.equal(annotation(blameFor(blame, 1, "one")!), "dev · trunk@7");
  assert.equal(annotation(blameFor(blame, 2, "two")!), "dev · bbbbbbbbbbbb");
  assert.equal(blameFor(blame, 1, "one, edited"), undefined);
  assert.equal(blameFor(blame, 3, ""), undefined);
});

test("trunk health shows green quietly and anything else as an alert", () => {
  const base = { since: 0, open: [], failing_steps: [], frozen: false, frozen_reason: null };
  assert.equal(healthView({ ...base, state: "green" }).alert, false);
  const red = healthView({ ...base, state: "red", failing_steps: ["//app:test"] });
  assert.ok(red.alert && red.tooltip.includes("//app:test"));
  const frozen = healthView({ ...base, state: "green", frozen: true, frozen_reason: "release" });
  assert.ok(frozen.alert && frozen.text.includes("frozen") && frozen.tooltip.includes("release"));
});

test("the inbox names the owner only for changes waiting on you", () => {
  const entry = { change: "ch_1", owner: "ann", status: "open", approvals: 1, open_threads: 2 };
  const s = inboxSections({ waiting: [entry], yours: [{ ...entry, change: "ch_2" }] });
  assert.equal(s.waiting[0].description, "open · 1 approval · 2 open threads · ann");
  assert.equal(s.yours[0].description, "open · 1 approval · 2 open threads");
});

test("threads are placed on 0-based lines; change threads are counted, not placed", () => {
  const comment = { author: dev, body: "why?", created_at: { rfc3339: "", millis: 0 } };
  const t: Threads = {
    change_id: "ch_1",
    threads: [
      { id: "t1", anchor: { kind: "lines", version: "v", path: "a.rs", start: 3, end: 5 }, comments: [comment], resolved: false },
      { id: "t2", anchor: { kind: "file", version: "v", path: "b.rs" }, comments: [comment], resolved: true },
      { id: "t3", anchor: { kind: "change" }, comments: [comment], resolved: false },
    ],
  };
  const placed = placedThreads(t);
  assert.deepEqual(
    placed.map((p) => [p.id, p.path, p.startLine, p.endLine, p.resolved]),
    [
      ["t1", "a.rs", 2, 4, false],
      ["t2", "b.rs", 0, 0, true],
    ],
  );
  assert.equal(openChangeThreads(t), 1);
});

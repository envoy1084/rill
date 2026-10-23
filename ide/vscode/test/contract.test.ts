// The extension reads documents the CLI prints; these are the CLI's own
// snapshot tests' outputs, so a schema change breaks this test too.

import { strict as assert } from "node:assert";
import * as fs from "node:fs";
import * as path from "node:path";
import { test } from "vitest";

import { parseDoc } from "../src/cli/run";
import type { Blame, Conflicts, Land, Status, Threads, TrunkHealth } from "../src/cli/schema";
import { annotation, blameFor } from "../src/model/blame";
import { healthView } from "../src/model/health";
import { fileGroups } from "../src/model/status";
import { placedThreads } from "../src/model/threads";

const SNAPSHOTS = path.join(__dirname, "../../../crates/clients/cli/tests/cli/snapshots");

function snapshot<T>(name: string, schema: string): T {
  const text = fs.readFileSync(path.join(SNAPSHOTS, `cli__schemas__${name}.snap`), "utf8");
  const body = text.split(/^---$/m).slice(2).join("---");
  return parseDoc<T>(body, schema);
}

test("status", () => {
  const s = snapshot<Status>("status", "status");
  const groups = fileGroups(s);
  assert.ok(groups.changes.length > 0);
  assert.ok(groups.changes.every((e) => ["A", "M", "D"].includes(e.letter)));
  assert.equal(typeof s.conflicts, "number");
});

test("blame", () => {
  const b = snapshot<Blame>("blame", "blame");
  const first = b.lines[0];
  const line = blameFor(b, first.line_no, first.text.replace(/\n$/, ""));
  assert.ok(line);
  assert.match(annotation(line), /^dev · trunk@\d+$/);
});

test("trunk health", () => {
  const h = snapshot<TrunkHealth>("trunk_health", "trunk.health");
  assert.equal(healthView(h.health).alert, false);
});

test("review threads, conflicts and land", () => {
  assert.deepEqual(placedThreads(snapshot<Threads>("review_threads", "review.threads")), []);
  assert.deepEqual(snapshot<Conflicts>("conflicts", "conflicts").conflicts, []);
  const land = snapshot<Land>("land", "land");
  assert.equal(land.status, "landed");
  assert.equal(typeof land.seq, "number");
});

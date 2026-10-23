import { strict as assert } from "node:assert";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { test } from "vitest";

import { parseDoc, parseError, RillError, run } from "../src/cli/run";

/** A stand-in `rill` that echoes its arguments or fails like the CLI. */
function fakeRill(script: string): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "fake-rill-"));
  const bin = path.join(dir, "rill");
  fs.writeFileSync(bin, `#!/usr/bin/env node\n${script}\n`, { mode: 0o755 });
  return bin;
}

test("a command's result is returned when its schema matches", async () => {
  const bin = fakeRill(
    `process.stdout.write(JSON.stringify({ schema: "rill.cli.v1.status", result: { args: process.argv.slice(2) } }));`,
  );
  const out = await run<{ args: string[] }>({ binary: bin, cwd: os.tmpdir() }, ["status"], "status");
  assert.deepEqual(out.args, ["--json", "--no-pager", "-C", os.tmpdir(), "status"]);
  await assert.rejects(run({ binary: bin, cwd: os.tmpdir() }, ["status"], "diff"), /expected rill.cli.v1.diff/);
});

test("a failure carries the CLI's error code, message, exit and what is missing", async () => {
  const bin = fakeRill(`
    process.stderr.write("WARN something\\n");
    process.stderr.write(JSON.stringify({ schema: "rill.cli.v1.error", error: {
      code: "failed_precondition", reason: "POLICY_DENIED", message: "needs review", missing: ["Review+2"], exit: 4 } }) + "\\n");
    process.exit(4);`);
  await assert.rejects(run({ binary: bin, cwd: os.tmpdir() }, ["land"], "land"), (e: unknown) => {
    assert.ok(e instanceof RillError);
    assert.equal(e.code, "failed_precondition");
    assert.equal(e.exit, 4);
    assert.deepEqual(e.missing, ["Review+2"]);
    return true;
  });
});

test("a missing binary says how to fix it", async () => {
  await assert.rejects(
    run({ binary: "/nonexistent/rill", cwd: os.tmpdir() }, ["status"], "status"),
    (e: unknown) => e instanceof RillError && e.code === "not_installed",
  );
});

test("output that is not a document is an error, not a crash", () => {
  assert.throws(() => parseDoc("rill: oops", "status"), /other than JSON/);
  assert.equal(parseError("plain failure\n", 3).message, "plain failure");
});

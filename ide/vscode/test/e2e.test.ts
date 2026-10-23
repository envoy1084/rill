// The extension's commands against a real `rill dev` server and client
// daemon, so every schema name and argument it uses is checked against
// the CLI. Runs when RILL_BIN names a built `rill`.

import { strict as assert } from "node:assert";
import { execFile, spawn, type ChildProcess } from "node:child_process";
import * as fs from "node:fs";
import * as net from "node:net";
import * as os from "node:os";
import * as path from "node:path";
import { afterAll, beforeAll, test } from "vitest";
import { promisify } from "node:util";

import { Rill } from "../src/cli/rill";

const BIN = process.env.RILL_BIN;
const exec = promisify(execFile);

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const s = net.createServer().listen(0, "127.0.0.1", () => {
      const port = (s.address() as net.AddressInfo).port;
      s.close(() => resolve(port));
    });
    s.on("error", reject);
  });
}

let server: ChildProcess | undefined;
let dir = "";
let env: NodeJS.ProcessEnv = {};

async function rill(...args: string[]): Promise<string> {
  return (await exec(BIN!, ["--json", ...args], { cwd: dir, env })).stdout;
}

beforeAll(async () => {
  if (!BIN) {
    return;
  }
  dir = fs.mkdtempSync(path.join(os.tmpdir(), "rill-vscode-e2e-"));
  const [grpc, http] = [await freePort(), await freePort()];
  env = {
    PATH: process.env.PATH,
    HOME: dir,
    NO_COLOR: "1",
    RILL_CONFIG: path.join(dir, "no-config.toml"),
    RILL_ENDPOINT: `http://127.0.0.1:${grpc}`,
    RILL_DAEMON_DIR: path.join(dir, "daemon"),
  };
  server = spawn(BIN, ["dev", "--memory", "--listen-grpc", `127.0.0.1:${grpc}`, "--listen-http", `127.0.0.1:${http}`], {
    env,
    stdio: "ignore",
  });
  for (let i = 0; ; i++) {
    try {
      await rill("repo", "list");
      break;
    } catch (e) {
      if (i > 100) {
        throw e;
      }
      await new Promise((r) => setTimeout(r, 100));
    }
  }
});

afterAll(async () => {
  if (!BIN) {
    return;
  }
  await rill("daemon", "stop").catch(() => undefined);
  server?.kill();
  fs.rmSync(dir, { recursive: true, force: true });
});

test.skipIf(!BIN)("the extension's commands work against a real checkout", async () => {
  await rill("repo", "create", "e2e");
  await rill("clone", "e2e", path.join(dir, "co"), "--eager");
  const co = path.join(dir, "co");
  const r = new Rill({ binary: BIN!, cwd: co, env });

  fs.writeFileSync(path.join(co, "README.md"), "hello\n");
  const status = await r.status();
  assert.deepEqual(status.changes, [{ path: "README.md", kind: "added" }]);
  assert.deepEqual((await r.conflicts()).conflicts, []);

  await r.commit("Add a readme");
  const change = (await r.status()).working_change ?? "";
  assert.ok(change.startsWith("ch_"), JSON.stringify(await r.status()));
  await r.describe("Add a readme, described");
  await r.submit();

  const opened = await r.comment(change, "why hello?", "README.md", 1);
  await r.reply(change, opened.thread_id, "because");
  const resolved = await r.resolveThread(change, opened.thread_id, false);
  assert.equal(resolved.resolved, true);
  const threads = await r.threads(change);
  assert.equal(threads.threads[0].comments.length, 2);

  const inbox = await r.inbox();
  assert.equal(inbox.yours[0].change, change);
  await rill("-C", co, "approve");
  await rill("-C", co, "review", "vote", change, "Verified", "+1");
  const land = await r.land(60);
  assert.equal(land.status, "landed", JSON.stringify(land));

  assert.equal((await r.trunkHealth()).health.state, "green");
  const blame = await r.blame("README.md");
  assert.equal(blame.lines[0].trunk_seq, land.seq);
  const cat = await r.cat("README.md", "trunk");
  assert.equal(cat.text, "hello\n");
  await r.sync();
});

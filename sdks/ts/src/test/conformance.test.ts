// The scenarios of sdks/conformance/README.md, against the server at
// RILL_ENDPOINT (`rill dev`); skipped when it is unset.

import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { test } from "node:test";

import { Code, ConnectError } from "@connectrpc/connect";

import { FileKind } from "../gen/rill/v1/types_pb.js";
import { LandStatus } from "../gen/rill/v1/landing_pb.js";
import { createRillClient, isReason, names, retry, type RillClient } from "../index.js";

const endpoint = process.env.RILL_ENDPOINT;
const skip = endpoint ? false : "RILL_ENDPOINT is not set";

async function setup(): Promise<[RillClient, string]> {
  const client = createRillClient(endpoint!);
  const id = `conf-${randomBytes(4).toString("hex")}`;
  await client.repos.createRepo({ requestId: names.requestId(), parent: names.org(), repoId: id });
  return [client, names.repo(id)];
}

const setFile = (path: string, text: string) => ({
  path,
  op: {
    case: "setContent" as const,
    value: {
      content: { ref: { case: "inline" as const, value: new TextEncoder().encode(text) } },
      kind: FileKind.REGULAR,
    },
  },
});

const onRevision = (revision: string) => ({ base: { case: "revision" as const, value: revision } });

async function rejection(call: Promise<unknown>): Promise<ConnectError> {
  try {
    await call;
  } catch (err) {
    return ConnectError.from(err);
  }
  assert.fail("the call succeeded");
}

test("resources and errors", { skip }, async () => {
  const [client, repo] = await setup();
  const got = await client.repos.getRepo({ name: repo });
  assert.equal(got.repo?.name, repo);
  const err = await rejection(client.repos.getRepo({ name: names.repo("absent") }));
  assert.equal(err.code, Code.NotFound);
  assert.ok(isReason(err, "NOT_FOUND"));
});

test("writes read back and retries are idempotent", { skip }, async () => {
  const [client, repo] = await setup();
  const requestId = names.requestId();
  const write = (message: string) =>
    client.changes.applyEdits({
      requestId,
      parent: repo,
      base: onRevision("trunk"),
      message,
      edits: [setFile("hello.txt", "hello\n")],
    });
  const first = await write("hello");
  const revision = names.revision(first.info!.hash);
  const file = await client.content.readFile({ repo, revision, path: "hello.txt" });
  assert.equal(new TextDecoder().decode(file.data), "hello\n");
  const dir = await client.content.listDirectory({ repo, revision });
  assert.deepEqual(dir.entries.map((e) => e.name), ["hello.txt"]);
  const again = await write("hello");
  assert.deepEqual(again.info!.hash, first.info!.hash);
  const reused = await rejection(write("another"));
  assert.equal(reused.code, Code.InvalidArgument);
  assert.ok(isReason(reused, "REQUEST_ID_REUSED"));
});

test("histories page", { skip }, async () => {
  const [client, repo] = await setup();
  const first = await client.changes.applyEdits({
    parent: repo,
    base: onRevision("trunk"),
    message: "one",
    edits: [setFile("a.txt", "1\n")],
  });
  const second = await client.changes.applyEdits({
    parent: repo,
    base: onRevision(names.revision(first.info!.hash)),
    message: "two",
    edits: [setFile("a.txt", "2\n")],
  });
  const revision = names.revision(second.info!.hash);
  const page = await client.content.listHistory({ repo, revision, pageSize: 1 });
  assert.equal(page.versions.length, 1);
  assert.notEqual(page.nextPageToken, "");
  const rest = await client.content.listHistory({
    repo,
    revision,
    pageSize: 1,
    pageToken: page.nextPageToken,
  });
  assert.deepEqual(rest.versions[0].hash, first.info!.hash);
  assert.equal(rest.nextPageToken, "");
});

test("a reviewed change lands and filters find it", { skip }, async () => {
  const [client, repo] = await setup();
  const written = await client.changes.applyEdits({
    parent: repo,
    base: onRevision("trunk"),
    message: "hello",
    edits: [setFile("hello.txt", "hello\n")],
  });
  const change = written.info!.changeId;
  await client.reviews.submitChange({ change: names.change(repo, change) });
  await client.reviews.vote({ review: names.review(repo, change), label: "Verified", value: 1 });
  await client.reviews.vote({ review: names.review(repo, change), label: "Review", value: 2 });
  const land = await retry(() => client.landing.requestLand({ change: names.change(repo, change) }));
  const deadline = Date.now() + 30_000;
  for (;;) {
    const got = await client.landing.getLandRequest({ name: land.landRequest!.name });
    if (got.landRequest!.status === LandStatus.LANDED) {
      assert.equal(got.landRequest!.trunkPosition, 1n);
      break;
    }
    assert.ok(Date.now() < deadline, "did not land");
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const head = await client.repos.getTrunkPosition({ name: names.trunkPosition(repo, "head") });
  assert.equal(head.landing!.position, 1n);
  const landed = await client.reviews.listReviews({ parent: repo, filter: 'status = "landed"' });
  assert.deepEqual(landed.reviews.map((r) => r.changeId), [change]);
  const bad = await rejection(client.reviews.listReviews({ parent: repo, filter: 'colour = "red"' }));
  assert.equal(bad.code, Code.InvalidArgument);
  assert.ok(isReason(bad, "INVALID_FILTER"));
});

test("etags guard updates", { skip }, async () => {
  const [client, repo] = await setup();
  const { policy } = await client.policies.getPolicy({ name: names.policy(repo, "submit") });
  assert.ok(policy?.etag);
  const update = (etag: string) =>
    client.policies.updatePolicy({ policy: { name: policy.name, text: policy.text }, etag });
  const stale = await rejection(update("stale"));
  assert.equal(stale.code, Code.Aborted);
  assert.ok(isReason(stale, "ETAG_MISMATCH"));
  await update(policy.etag);
});

test("retry gives up on errors that are not transient", async () => {
  let calls = 0;
  const err = await rejection(
    retry(async () => {
      calls++;
      throw new ConnectError("gone", Code.NotFound);
    }),
  );
  assert.equal(err.code, Code.NotFound);
  assert.equal(calls, 1);
});

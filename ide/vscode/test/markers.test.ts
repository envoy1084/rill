import { strict as assert } from "node:assert";
import { test } from "vitest";

import { parseConflicts } from "../src/model/markers";

test("a two-sided conflict yields both sides and the base", () => {
  const text = [
    "a",
    "<<<<<<< Conflict 1 of 1 (2 sides)",
    "%%%%%%% Changes from base #1 to side #1",
    "-b",
    "+L",
    "+++++++ Contents of side #2",
    "R",
    ">>>>>>> Conflict 1 of 1 ends",
    "c",
    "",
  ].join("\n");
  const [region, ...rest] = parseConflicts(text);
  assert.equal(rest.length, 0);
  assert.equal(region.startLine, 1);
  assert.equal(region.endLine, 7);
  assert.deepEqual(region.sides, ["L\n", "R\n"]);
  assert.deepEqual(region.bases, ["b\n"]);
});

test("context lines belong to the side and the base", () => {
  const text = [
    "<<<<<<< Conflict 1 of 1 (2 sides)",
    "%%%%%%% Changes from base #1 to side #1",
    " keep",
    "-old",
    "+new",
    "+++++++ Contents of side #2",
    "keep",
    "other",
    ">>>>>>> Conflict 1 of 1 ends",
  ].join("\n");
  const [region] = parseConflicts(text);
  assert.deepEqual(region.sides, ["keep\nnew\n", "keep\nother\n"]);
  assert.deepEqual(region.bases, ["keep\nold\n"]);
});

test("three sides, longer markers, a missing final newline and two regions", () => {
  const text = [
    "<<<<<<<<< Conflict 1 of 2 (3 sides)",
    "%%%%%%%%% Changes from base #1 to side #1",
    "-x",
    "+one",
    "%%%%%%%%% Changes from base #2 to side #2",
    "-x",
    "+two",
    "+++++++++ Contents of side #3",
    "<<<<<<< not a marker here",
    ">>>>>>>>> Conflict 1 of 2 ends",
    "between",
    "<<<<<<< Conflict 2 of 2 (2 sides)",
    "%%%%%%% Changes from base #1 to side #1",
    "-p",
    "+q",
    "\\ No newline at end of file",
    "+++++++ Contents of side #2",
    "r",
    ">>>>>>> Conflict 2 of 2 ends",
  ].join("\n");
  const regions = parseConflicts(text);
  assert.equal(regions.length, 2);
  assert.deepEqual(regions[0].sides, ["one\n", "two\n", "<<<<<<< not a marker here\n"]);
  assert.deepEqual(regions[0].bases, ["x\n", "x\n"]);
  assert.equal(regions[1].startLine, 11);
  assert.deepEqual(regions[1].sides, ["q", "r\n"]);
});

test("an unterminated region and plain text are not conflicts", () => {
  assert.deepEqual(parseConflicts("a\n<<<<<<< Conflict 1 of 1 (2 sides)\n%%%%%%% x\n+y\n"), []);
  assert.deepEqual(parseConflicts("<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> branch\n"), []);
});

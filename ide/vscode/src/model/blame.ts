// Inline blame text for one line.

import type { Blame, BlameLine } from "../cli/schema";

/** The blame of 1-based `lineNo`, if the file still has that line as blamed. */
export function blameFor(blame: Blame, lineNo: number, currentText: string): BlameLine | undefined {
  const line = blame.lines[lineNo - 1]?.line_no === lineNo
    ? blame.lines[lineNo - 1]
    : blame.lines.find((l) => l.line_no === lineNo);
  if (!line || stripEol(line.text) !== currentText) {
    return undefined;
  }
  return line;
}

/** `ann · trunk@12`, or the short version for a change not yet landed. */
export function annotation(line: BlameLine): string {
  const where = line.trunk_seq != null ? `trunk@${line.trunk_seq}` : line.version.slice(0, 12);
  return `${line.author.display} · ${where}`;
}

function stripEol(text: string): string {
  return text.replace(/\r?\n$/, "");
}

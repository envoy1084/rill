// Conflict regions in a file as Rill writes them (Jujutsu style):
//
//   <<<<<<< Conflict 1 of 1 (2 sides)
//   %%%%%%% Changes from base #1 to side #1
//   -base line
//   +side 1 line
//    context
//   +++++++ Contents of side #2
//   side 2 line
//   >>>>>>> Conflict 1 of 1 ends
//
// Each `%%%%%%%` section is a diff from a base to one side; the
// `+++++++` section is the last side verbatim. Markers are seven or more
// characters, the length set by the opening line.

export interface ConflictRegion {
  /** 0-based line of the opening marker. */
  startLine: number;
  /** 0-based line of the closing marker. */
  endLine: number;
  /** Each side's content, in order. */
  sides: string[];
  /** Each base's content, in order. */
  bases: string[];
}

const NO_NEWLINE = "\\ No newline at end of file";

class Text {
  private lines: string[] = [];
  private bare = false;

  push(line: string): void {
    this.lines.push(line);
  }

  noNewlineAtEnd(): void {
    this.bare = true;
  }

  toString(): string {
    if (this.lines.length === 0) {
      return "";
    }
    return this.lines.join("\n") + (this.bare ? "" : "\n");
  }
}

export function parseConflicts(text: string): ConflictRegion[] {
  const lines = text.split("\n");
  const regions: ConflictRegion[] = [];
  let i = 0;
  while (i < lines.length) {
    const open = /^(<{7,}) Conflict \d+ of \d+/.exec(lines[i]);
    if (!open) {
      i++;
      continue;
    }
    const n = open[1].length;
    const close = ">".repeat(n) + " ";
    const diff = "%".repeat(n) + " ";
    const snapshot = "+".repeat(n) + " ";
    const sides: Text[] = [];
    const bases: Text[] = [];
    let mode: "none" | "diff" | "snapshot" = "none";
    let last: Text[] = [];
    let j = i + 1;
    for (; j < lines.length && !lines[j].startsWith(close); j++) {
      const line = lines[j];
      if (line.startsWith(diff)) {
        sides.push(new Text());
        bases.push(new Text());
        mode = "diff";
      } else if (line.startsWith(snapshot)) {
        sides.push(new Text());
        mode = "snapshot";
      } else if (line === NO_NEWLINE) {
        last.forEach((t) => t.noNewlineAtEnd());
      } else if (mode === "snapshot") {
        sides[sides.length - 1].push(line);
        last = [sides[sides.length - 1]];
      } else if (mode === "diff") {
        const side = sides[sides.length - 1];
        const base = bases[bases.length - 1];
        const body = line.slice(1);
        last = line.startsWith("-") ? [base] : line.startsWith("+") ? [side] : [side, base];
        last.forEach((t) => t.push(body));
      }
    }
    if (j >= lines.length) {
      break;
    }
    regions.push({
      startLine: i,
      endLine: j,
      sides: sides.map(String),
      bases: bases.map(String),
    });
    i = j + 1;
  }
  return regions;
}

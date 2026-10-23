// Running `rill --json` and reading its documents: `{schema, result}` on
// stdout, or `{schema: "rill.cli.v1.error", error}` on stderr.

import { execFile } from "node:child_process";

import type { CliErrorBody } from "./schema";

export const SCHEMA_PREFIX = "rill.cli.v1";

/** A failed `rill` command, with the CLI's stable error code. */
export class RillError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly exit: number,
    readonly missing: string[] = [],
  ) {
    super(message);
    this.name = "RillError";
  }
}

/** How to run the CLI. */
export interface Runner {
  binary: string;
  cwd: string;
  env?: NodeJS.ProcessEnv;
}

/** Run `rill --json <args>` in the runner's checkout and return `result`. */
export function run<T>(runner: Runner, args: string[], schema: string): Promise<T> {
  const full = ["--json", "--no-pager", "-C", runner.cwd, ...args];
  return new Promise((resolve, reject) => {
    execFile(
      runner.binary,
      full,
      { cwd: runner.cwd, env: runner.env ?? process.env, maxBuffer: 64 << 20 },
      (err, stdout, stderr) => {
        if (err) {
          const missing = (err as NodeJS.ErrnoException).code === "ENOENT";
          reject(
            missing
              ? new RillError("not_installed", `${runner.binary} was not found; set rill.path`, 127)
              : parseError(stderr, typeof err.code === "number" ? err.code : 1),
          );
          return;
        }
        try {
          resolve(parseDoc<T>(stdout, schema));
        } catch (e) {
          reject(e);
        }
      },
    );
  });
}

/** The `result` of a document, checking it has the expected schema. */
export function parseDoc<T>(stdout: string, schema: string): T {
  let doc: { schema?: unknown; result?: unknown };
  try {
    doc = JSON.parse(stdout);
  } catch {
    throw new RillError("bad_output", `rill printed something other than JSON: ${stdout.slice(0, 200)}`, 1);
  }
  const want = `${SCHEMA_PREFIX}.${schema}`;
  if (doc.schema !== want) {
    throw new RillError("bad_output", `expected ${want}, got ${String(doc.schema)}`, 1);
  }
  return doc.result as T;
}

/** The error a failed command printed, or a generic one. */
export function parseError(stderr: string, exit: number): RillError {
  const lines = stderr.trim().split("\n").reverse();
  for (const line of lines) {
    try {
      const doc = JSON.parse(line) as { schema?: string; error?: CliErrorBody };
      if (doc.schema === `${SCHEMA_PREFIX}.error` && doc.error) {
        const e = doc.error;
        return new RillError(e.code, e.message, e.exit ?? exit, e.missing ?? []);
      }
    } catch {
      // Log lines before the error document are not JSON.
    }
  }
  return new RillError("error", stderr.trim() || `rill exited with ${exit}`, exit);
}

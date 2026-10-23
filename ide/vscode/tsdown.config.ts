import { defineConfig } from "tsdown";

// One CommonJS bundle VS Code loads; `vscode` is provided by the editor.
export default defineConfig({
  entry: { extension: "src/extension.ts" },
  format: "cjs",
  outExtensions: () => ({ js: ".cjs" }),
  platform: "node",
  target: "node22",
  external: ["vscode"],
  sourcemap: true,
  dts: false,
});

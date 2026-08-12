#!/usr/bin/env node
/**
 * Fill tests/fixtures/search/parse-cases.json `expected` from TypeScript parseSearchQuery.
 *
 * Deprecated with web-next: the TypeScript parser only backs that UI. The Vite
 * SPA sends query strings to the server and Rust parses them.
 *
 * Usage (from repo root):
 *   node scripts/deprecated/regen-search-goldens.mjs
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..", "..");
const tsxCli = path.join(root, "web-next", "node_modules", "tsx", "dist", "cli.mjs");
const worker = path.join(__dirname, "regen-search-goldens-worker.ts");

if (!fs.existsSync(tsxCli)) {
  console.error(
    "regen-search-goldens: web-next/node_modules/tsx missing. Run: cd web-next && npm ci",
  );
  process.exit(1);
}

const result = spawnSync(process.execPath, [tsxCli, worker], {
  cwd: root,
  stdio: "inherit",
});
process.exit(result.status ?? 1);

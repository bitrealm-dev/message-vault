#!/usr/bin/env node
/**
 * Fill fixtures/search/parse-cases.json `expected` from TypeScript parseSearchQuery.
 *
 * Usage (from repo root):
 *   node scripts/regen-search-goldens.mjs
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const tsxCli = path.join(root, "web", "node_modules", "tsx", "dist", "cli.mjs");
const worker = path.join(__dirname, "regen-search-goldens-worker.ts");

if (!fs.existsSync(tsxCli)) {
  console.error(
    "regen-search-goldens: web/node_modules/tsx missing. Run: cd web && npm ci",
  );
  process.exit(1);
}

const result = spawnSync(process.execPath, [tsxCli, worker], {
  cwd: root,
  stdio: "inherit",
});
process.exit(result.status ?? 1);

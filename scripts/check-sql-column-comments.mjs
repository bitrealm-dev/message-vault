#!/usr/bin/env node
/**
 * Fail if any CREATE TABLE / CREATE VIRTUAL TABLE column in schema/sql
 * lacks a `--` comment on the immediately preceding non-empty line.
 *
 * Usage: node scripts/check-sql-column-comments.mjs
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sqlDir = path.join(root, "schema", "sql");
const FILES = [
  "accounts.sql",
  "messages.sql",
  "staging.sql",
  "contacts.sql",
  "fts_virtual.sql",
];

const COLUMN_RE =
  /^\s{4}([a-z_][a-z0-9_]*)\s+(INTEGER|TEXT|REAL|BLOB)\b/i;
const FTS_COLUMN_RE = /^\s{4}([a-z_][a-z0-9_]*),?\s*$/i;
const FTS_OPTION_RE = /^\s{4}(content|tokenize)\s*=/i;
const CONSTRAINT_START =
  /^\s{4}(UNIQUE|PRIMARY\s+KEY|FOREIGN\s+KEY|CHECK|CONSTRAINT)\b/i;

function previousNonEmpty(lines, index) {
  for (let i = index - 1; i >= 0; i--) {
    const t = lines[i].trim();
    if (t.length === 0) continue;
    return { text: t, line: i + 1 };
  }
  return null;
}

function checkFile(file) {
  const full = path.join(sqlDir, file);
  const lines = fs.readFileSync(full, "utf8").split(/\r?\n/);
  const errors = [];
  let inCreate = false;
  let inFts = false;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    if (/^CREATE\s+TABLE\b/i.test(trimmed)) {
      inCreate = true;
      inFts = false;
      continue;
    }
    if (/^CREATE\s+VIRTUAL\s+TABLE\b/i.test(trimmed)) {
      inCreate = true;
      inFts = /USING\s+fts5\s*\(/i.test(trimmed);
      continue;
    }
    if (inCreate && trimmed === ");") {
      inCreate = false;
      inFts = false;
      continue;
    }
    if (!inCreate) continue;
    if (CONSTRAINT_START.test(line)) continue;
    if (inFts && FTS_OPTION_RE.test(line)) continue;

    let col = null;
    if (inFts) {
      const m = line.match(FTS_COLUMN_RE);
      if (m) col = m[1];
    } else {
      const m = line.match(COLUMN_RE);
      if (m) col = m[1];
    }
    if (!col) continue;

    const prev = previousNonEmpty(lines, i);
    if (!prev || !prev.text.startsWith("--")) {
      errors.push(`${file}:${i + 1}:${col}`);
    }
  }
  return errors;
}

const all = FILES.flatMap(checkFile);
if (all.length === 0) {
  console.log("check-sql-column-comments: OK");
  process.exit(0);
}
console.error("Missing column comments:");
for (const e of all) console.error(`  ${e}`);
process.exit(1);

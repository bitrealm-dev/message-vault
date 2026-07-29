import assert from "node:assert/strict";
import { describe, it } from "node:test";
import Database from "better-sqlite3";

import {
  ACCOUNTS_DEFAULT_READ_ONLY_META_KEY,
  ensureVaultSchema,
} from "./vaultSchema";

describe("accounts default read-only migration", () => {
  it("defaults new accounts to read-only", () => {
    const db = new Database(":memory:");
    ensureVaultSchema(db);
    db.prepare(`INSERT INTO accounts (id, username) VALUES (?, ?)`).run(
      "11111111-1111-1111-1111-111111111111",
      "fresh",
    );
    const row = db
      .prepare(`SELECT read_only FROM accounts WHERE id = ?`)
      .get("11111111-1111-1111-1111-111111111111") as { read_only: number };
    assert.equal(row.read_only, 1);
    db.close();
  });

  it("locks existing accounts once and preserves later unlocks", () => {
    const db = new Database(":memory:");
    db.exec(`
      CREATE TABLE accounts (
        id TEXT PRIMARY KEY,
        username TEXT NOT NULL UNIQUE COLLATE NOCASE,
        read_only INTEGER NOT NULL DEFAULT 0
      );
    `);
    db.prepare(
      `INSERT INTO accounts (id, username, read_only) VALUES (?, ?, 0)`,
    ).run("11111111-1111-1111-1111-111111111111", "alice");

    ensureVaultSchema(db);
    const locked = db
      .prepare(`SELECT read_only FROM accounts WHERE id = ?`)
      .get("11111111-1111-1111-1111-111111111111") as { read_only: number };
    assert.equal(locked.read_only, 1);
    const marker = db
      .prepare(`SELECT value FROM schema_meta WHERE key = ?`)
      .get(ACCOUNTS_DEFAULT_READ_ONLY_META_KEY) as { value: string };
    assert.equal(marker.value, "1");

    db.prepare(`UPDATE accounts SET read_only = 0 WHERE id = ?`).run(
      "11111111-1111-1111-1111-111111111111",
    );
    ensureVaultSchema(db);
    const stillUnlocked = db
      .prepare(`SELECT read_only FROM accounts WHERE id = ?`)
      .get("11111111-1111-1111-1111-111111111111") as { read_only: number };
    assert.equal(stillUnlocked.read_only, 0);
    db.close();
  });
});

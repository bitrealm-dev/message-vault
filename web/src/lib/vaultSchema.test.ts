import assert from "node:assert/strict";
import { describe, it } from "node:test";
import Database from "better-sqlite3";

import {
  ACCOUNTS_DEFAULT_READ_ONLY_META_KEY,
  CONTACT_STATUS_LABELS_META_KEY,
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

describe("contact status label migration", () => {
  it("converts legacy exclude values once into ordinary labels", () => {
    const db = new Database(":memory:");
    ensureVaultSchema(db);
    const accountId = "11111111-1111-1111-1111-111111111111";
    db.prepare(`INSERT INTO accounts (id, username) VALUES (?, ?)`).run(
      accountId,
      "alice",
    );
    db.prepare(
      `INSERT INTO contacts (account_id, first_name, exclude) VALUES (?, ?, ?)`,
    ).run(accountId, "Ada", 0);
    db.prepare(
      `INSERT INTO contacts (account_id, first_name, exclude) VALUES (?, ?, ?)`,
    ).run(accountId, "Grace", 1);
    db.prepare(`DELETE FROM schema_meta WHERE key = ?`).run(
      CONTACT_STATUS_LABELS_META_KEY,
    );

    ensureVaultSchema(db);

    const rows = db
      .prepare(
        `SELECT c.first_name AS name, c.exclude, cl.name AS label
         FROM contacts c
         JOIN contact_label_members clm ON clm.contact_id = c.id
         JOIN contact_labels cl ON cl.id = clm.label_id
         ORDER BY c.first_name`,
      )
      .all() as Array<{ name: string; exclude: number; label: string }>;
    assert.deepEqual(rows, [
      { name: "Ada", exclude: 0, label: "Active" },
      { name: "Grace", exclude: 0, label: "Inactive" },
    ]);

    db.prepare(
      `DELETE FROM contact_label_members
       WHERE contact_id = (SELECT id FROM contacts WHERE first_name = 'Ada')`,
    ).run();
    ensureVaultSchema(db);
    const activeMemberships = db
      .prepare(
        `SELECT COUNT(*) AS n
         FROM contact_label_members clm
         JOIN contacts c ON c.id = clm.contact_id
         WHERE c.first_name = 'Ada'`,
      )
      .get() as { n: number };
    assert.equal(activeMemberships.n, 0);
    db.close();
  });
});

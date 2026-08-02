import assert from "node:assert/strict";
import { describe, it } from "node:test";
import Database from "better-sqlite3";

import {
  ACCOUNTS_DEFAULT_READ_ONLY_META_KEY,
  CONTACT_STATUS_LABELS_META_KEY,
  CONTACTS_PREFERRED_NAME_META_KEY,
  VAULT_OWNERS_INTO_ACCOUNTS_META_KEY,
  ensureVaultSchema,
} from "./vaultSchema";

describe("accounts default read-only migration", () => {
  it("defaults new accounts to writable", () => {
    const db = new Database(":memory:");
    ensureVaultSchema(db);
    db.prepare(`INSERT INTO accounts (id, username) VALUES (?, ?)`).run(
      "11111111-1111-1111-1111-111111111111",
      "fresh",
    );
    const row = db
      .prepare(`SELECT read_only FROM accounts WHERE id = ?`)
      .get("11111111-1111-1111-1111-111111111111") as { read_only: number };
    assert.equal(row.read_only, 0);
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

describe("vault owners into accounts migration", () => {
  it("copies names and phones then drops vault_owner tables", () => {
    const db = new Database(":memory:");
    const accountId = "11111111-1111-1111-1111-111111111111";
    db.exec(`
      CREATE TABLE accounts (
        id TEXT PRIMARY KEY,
        username TEXT NOT NULL UNIQUE COLLATE NOCASE,
        read_only INTEGER NOT NULL DEFAULT 0
      );
      CREATE TABLE account_emails (
        account_id TEXT NOT NULL,
        email TEXT NOT NULL UNIQUE COLLATE NOCASE,
        is_primary INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (account_id, email)
      );
      CREATE TABLE vault_owners (
        account_id TEXT PRIMARY KEY,
        first_name TEXT NOT NULL DEFAULT '',
        last_name TEXT NOT NULL DEFAULT '',
        display_name TEXT NOT NULL
      );
      CREATE TABLE vault_owner_phones (
        account_id TEXT NOT NULL,
        phone TEXT NOT NULL,
        PRIMARY KEY (account_id, phone)
      );
      CREATE TABLE vault_owner_emails (
        account_id TEXT NOT NULL,
        email TEXT NOT NULL,
        PRIMARY KEY (account_id, email)
      );
    `);
    db.prepare(`INSERT INTO accounts (id, username) VALUES (?, ?)`).run(
      accountId,
      "alice",
    );
    db.prepare(
      `INSERT INTO account_emails (account_id, email, is_primary) VALUES (?, ?, 1)`,
    ).run(accountId, "alice@example.com");
    db.prepare(
      `INSERT INTO vault_owners (account_id, first_name, last_name, display_name)
       VALUES (?, ?, ?, ?)`,
    ).run(accountId, "Ann", "Lee", "Ann Lee");
    db.prepare(
      `INSERT INTO vault_owner_phones (account_id, phone) VALUES (?, ?)`,
    ).run(accountId, "+15555550100");
    db.prepare(
      `INSERT INTO vault_owner_emails (account_id, email) VALUES (?, ?)`,
    ).run(accountId, "ann@example.com");

    ensureVaultSchema(db);

    const account = db
      .prepare(
        `SELECT first_name, last_name, preferred_name FROM accounts WHERE id = ?`,
      )
      .get(accountId) as {
      first_name: string;
      last_name: string;
      preferred_name: string | null;
    };
    assert.equal(account.first_name, "Ann");
    assert.equal(account.last_name, "Lee");
    assert.equal(account.preferred_name, "Ann Lee");

    const phone = db
      .prepare(`SELECT phone FROM account_phones WHERE account_id = ?`)
      .get(accountId) as { phone: string };
    assert.equal(phone.phone, "+15555550100");

    const emails = (
      db
        .prepare(`SELECT email FROM account_emails WHERE account_id = ? ORDER BY email`)
        .all(accountId) as Array<{ email: string }>
    ).map((r) => r.email);
    assert.deepEqual(emails, ["alice@example.com", "ann@example.com"]);

    const oldTables = db
      .prepare(
        `SELECT COUNT(*) AS n FROM sqlite_master
         WHERE type = 'table' AND name LIKE 'vault_owner%'`,
      )
      .get() as { n: number };
    assert.equal(oldTables.n, 0);

    const marker = db
      .prepare(`SELECT value FROM schema_meta WHERE key = ?`)
      .get(VAULT_OWNERS_INTO_ACCOUNTS_META_KEY) as { value: string };
    assert.equal(marker.value, "1");
    db.close();
  });
});

describe("contacts preferred_name migration", () => {
  it("adds preferred_name and backfills from first + last", () => {
    const db = new Database(":memory:");
    const accountId = "11111111-1111-1111-1111-111111111111";
    db.exec(`
      CREATE TABLE accounts (
        id TEXT PRIMARY KEY,
        username TEXT NOT NULL UNIQUE COLLATE NOCASE,
        read_only INTEGER NOT NULL DEFAULT 0
      );
      CREATE TABLE contacts (
        id INTEGER PRIMARY KEY,
        account_id TEXT NOT NULL,
        first_name TEXT,
        last_name TEXT,
        exclude INTEGER NOT NULL DEFAULT 0,
        preferred_handle TEXT
      );
    `);
    db.prepare(`INSERT INTO accounts (id, username) VALUES (?, ?)`).run(
      accountId,
      "alice",
    );
    db.prepare(
      `INSERT INTO contacts (account_id, first_name, last_name, preferred_handle)
       VALUES (?, ?, ?, ?)`,
    ).run(accountId, "Ann", "Lee", "+15555550100");

    ensureVaultSchema(db);

    const row = db
      .prepare(`SELECT preferred_name FROM contacts WHERE account_id = ?`)
      .get(accountId) as { preferred_name: string | null };
    assert.equal(row.preferred_name, "Ann Lee");
    const marker = db
      .prepare(`SELECT value FROM schema_meta WHERE key = ?`)
      .get(CONTACTS_PREFERRED_NAME_META_KEY) as { value: string };
    assert.equal(marker.value, "1");

    db.prepare(`UPDATE contacts SET preferred_name = NULL`).run();
    ensureVaultSchema(db);
    const still = db
      .prepare(`SELECT preferred_name FROM contacts WHERE account_id = ?`)
      .get(accountId) as { preferred_name: string | null };
    assert.equal(still.preferred_name, null);
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

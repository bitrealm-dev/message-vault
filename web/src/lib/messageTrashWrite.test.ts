import assert from "node:assert/strict";
import { describe, it } from "node:test";
import Database from "better-sqlite3";

import { setMessageTrashInDb } from "./messageTrashWrite";

function testDb(): Database.Database {
  const db = new Database(":memory:");
  db.exec(`
    CREATE TABLE accounts (id TEXT PRIMARY KEY);
    CREATE TABLE conversations (
      id INTEGER PRIMARY KEY,
      account_id TEXT NOT NULL,
      conversation_type TEXT NOT NULL
    );
    CREATE TABLE contacts (
      id INTEGER PRIMARY KEY,
      account_id TEXT NOT NULL
    );
    CREATE TABLE contact_handles (
      account_id TEXT NOT NULL,
      handle TEXT NOT NULL,
      contact_id INTEGER NOT NULL,
      PRIMARY KEY (account_id, handle)
    );
    INSERT INTO accounts (id) VALUES ('account-a'), ('account-b');
    INSERT INTO contacts (id, account_id) VALUES (10, 'account-a');
    INSERT INTO contact_handles (account_id, handle, contact_id)
      VALUES ('account-a', '+15555550123', 10);
    INSERT INTO conversations (id, account_id, conversation_type)
      VALUES
        (20, 'account-a', 'group'),
        (21, 'account-a', 'individual'),
        (30, 'account-b', 'group');
  `);
  return db;
}

describe("mixed message trash writes", () => {
  it("trashes and restores direct handles and group conversations together", () => {
    const db = testDb();
    try {
      const targets = {
        handles: [" +15555550123 ", "+15555550123"],
        conversationIds: [20, 20],
      };
      const trashed = setMessageTrashInDb(db, targets, true, "account-a");
      assert.deepEqual(trashed, {
        handles: ["+15555550123"],
        conversationIds: [20],
        count: 2,
      });
      assert.equal(
        (
          db.prepare(`SELECT COUNT(*) AS n FROM contacts WHERE id = 10`).get() as {
            n: number;
          }
        ).n,
        1,
      );
      assert.equal(
        (
          db.prepare(`SELECT COUNT(*) AS n FROM trashed_handles`).get() as {
            n: number;
          }
        ).n,
        1,
      );
      assert.equal(
        (
          db
            .prepare(`SELECT COUNT(*) AS n FROM trashed_conversations`)
            .get() as { n: number }
        ).n,
        1,
      );

      const restored = setMessageTrashInDb(db, targets, false, "account-a");
      assert.equal(restored.count, 2);
      assert.equal(
        (
          db
            .prepare(
              `SELECT
                 (SELECT COUNT(*) FROM trashed_handles) +
                 (SELECT COUNT(*) FROM trashed_conversations) AS n`,
            )
            .get() as { n: number }
        ).n,
        0,
      );
    } finally {
      db.close();
    }
  });

  it("rolls back the whole batch when a group target is invalid", () => {
    const db = testDb();
    try {
      assert.throws(
        () =>
          setMessageTrashInDb(
            db,
            {
              handles: ["+15555550123"],
              conversationIds: [21, 30],
            },
            true,
            "account-a",
          ),
        /group conversation 21 not found/,
      );
      assert.equal(
        (
          db.prepare(`SELECT COUNT(*) AS n FROM trashed_handles`).get() as {
            n: number;
          }
        ).n,
        0,
      );
      assert.equal(
        (
          db
            .prepare(`SELECT COUNT(*) AS n FROM trashed_conversations`)
            .get() as { n: number }
        ).n,
        0,
      );
    } finally {
      db.close();
    }
  });
});

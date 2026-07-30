import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";
import Database from "better-sqlite3";

import { runWithAccount } from "./accountScope";
import { createAccount, saveAccount } from "./accounts";
import { searchVault } from "./search";
import { dbPath } from "./paths";
import { ensureVaultSchema, MESSAGES_FTS_BACKFILL_META_KEY } from "./vaultSchema";

describe("vault search + FTS", () => {
  const prevVaultDb = process.env.VAULT_DB;
  const prevVaultDataDir = process.env.VAULT_DATA_DIR;
  let tmpDir = "";
  let accountId = "";

  before(() => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "vault-search-"));
    process.env.VAULT_DB = path.join(tmpDir, "vault.db");
    process.env.VAULT_DATA_DIR = path.join(tmpDir, "data");
    const account = createAccount({
      username: `search_${Date.now()}`,
      primaryEmail: `search_${Date.now()}@example.com`,
      firstName: "Search",
      lastName: "User",
      phone: "+15555550100",
    });
    accountId = account.id;
    assert.equal(account.read_only, true);

    const db = new Database(dbPath());
    try {
      ensureVaultSchema(db);
      const marker = db
        .prepare(`SELECT value FROM schema_meta WHERE key = ?`)
        .get(MESSAGES_FTS_BACKFILL_META_KEY) as { value: string } | undefined;
      assert.equal(marker?.value, "1");

      const insertConv = db.prepare(
        `INSERT INTO conversations (
           account_id, chat_identifier, service, conversation_type,
           group_title, exported_at, source_file
         ) VALUES (?, ?, 'iMessage', 'individual', NULL, NULL, 't.json')`,
      );
      const insertMsg = db.prepare(
        `INSERT INTO messages (
           conversation_id, account_id, source, guid, timestamp,
           is_from_me, sort_order, body, subject
         ) VALUES (?, ?, 'imessage', ?, ?, 0, 0, ?, NULL)`,
      );

      const daysAgo = (days: number) =>
        new Date(Date.now() - days * 86_400_000).toISOString();

      const ftsConvId = Number(
        insertConv.run(accountId, "+15555550999").lastInsertRowid,
      );
      insertMsg.run(
        ftsConvId,
        accountId,
        "g-search-1",
        "2021-06-01T12:00:00Z",
        "unique zebra pineapple vault",
      );

      // Long-known + still active: first ~10y ago, last ~5d ago.
      const activeConvId = Number(
        insertConv.run(accountId, "+15555551001").lastInsertRowid,
      );
      insertMsg.run(
        activeConvId,
        accountId,
        "g-active-1",
        daysAgo(3650),
        "hello from long ago",
      );
      insertMsg.run(
        activeConvId,
        accountId,
        "g-active-2",
        daysAgo(5),
        "still chatting recently",
      );

      // Stale + old: first ~10y ago, last ~400d ago.
      const staleConvId = Number(
        insertConv.run(accountId, "+15555551002").lastInsertRowid,
      );
      insertMsg.run(
        staleConvId,
        accountId,
        "g-stale-1",
        daysAgo(3650),
        "old friendship start",
      );
      insertMsg.run(
        staleConvId,
        accountId,
        "g-stale-2",
        daysAgo(400),
        "last contact a while back",
      );

      // Entirely recent: first and last within the last week.
      const recentConvId = Number(
        insertConv.run(accountId, "+15555551003").lastInsertRowid,
      );
      insertMsg.run(
        recentConvId,
        accountId,
        "g-recent-1",
        daysAgo(6),
        "brand new contact",
      );
      insertMsg.run(
        recentConvId,
        accountId,
        "g-recent-2",
        daysAgo(1),
        "just messaged yesterday",
      );
    } finally {
      db.close();
    }
  });

  after(() => {
    if (prevVaultDb === undefined) delete process.env.VAULT_DB;
    else process.env.VAULT_DB = prevVaultDb;
    if (prevVaultDataDir === undefined) delete process.env.VAULT_DATA_DIR;
    else process.env.VAULT_DATA_DIR = prevVaultDataDir;
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it("finds messages while the vault is read-only", () => {
    runWithAccount(accountId, () => {
      const result = searchVault("zebra");
      assert.ok(result.totalConversations >= 1);
      assert.ok(result.hits.some((h) => h.topMatch?.snippet.includes("zebra")));
    });
  });

  it("still searches after unlocking", () => {
    saveAccount(accountId, { read_only: false });
    runWithAccount(accountId, () => {
      const result = searchVault("pineapple");
      assert.ok(result.totalConversations >= 1);
    });
  });

  it("filters by last-contact: (last message on or before date)", () => {
    runWithAccount(accountId, () => {
      const cutoff = new Date(Date.now() - 30 * 86_400_000)
        .toISOString()
        .slice(0, 10);
      const result = searchVault(`last-contact:${cutoff}`);
      const handles = result.hits.map((h) => h.chatIdentifier);
      assert.ok(handles.includes("+15555551002"));
      assert.ok(!handles.includes("+15555551001"));
      assert.ok(!handles.includes("+15555551003"));
    });
  });

  it("filters by first-contact: (first message on or before date)", () => {
    runWithAccount(accountId, () => {
      const cutoff = new Date(Date.now() - 5 * 365 * 86_400_000)
        .toISOString()
        .slice(0, 10);
      const result = searchVault(`first-contact:${cutoff}`);
      const handles = result.hits.map((h) => h.chatIdentifier);
      assert.ok(handles.includes("+15555551001"));
      assert.ok(handles.includes("+15555551002"));
      assert.ok(!handles.includes("+15555551003"));
    });
  });

  it("combines last-contact: and first-contact:", () => {
    runWithAccount(accountId, () => {
      const lastCutoff = new Date(Date.now() - 30 * 86_400_000)
        .toISOString()
        .slice(0, 10);
      const firstCutoff = new Date(Date.now() - 5 * 365 * 86_400_000)
        .toISOString()
        .slice(0, 10);
      const result = searchVault(
        `last-contact:${lastCutoff} first-contact:${firstCutoff}`,
      );
      const handles = result.hits.map((h) => h.chatIdentifier);
      assert.ok(handles.includes("+15555551002"));
      assert.ok(!handles.includes("+15555551001"));
      assert.ok(!handles.includes("+15555551003"));
    });
  });
});

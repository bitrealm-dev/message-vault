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

      const conv = db
        .prepare(
          `INSERT INTO conversations (
             account_id, chat_identifier, service, conversation_type,
             group_title, exported_at, source_file
           ) VALUES (?, '+15555550999', 'iMessage', 'individual', NULL, NULL, 't.json')`,
        )
        .run(accountId);
      const conversationId = Number(conv.lastInsertRowid);
      db.prepare(
        `INSERT INTO messages (
           conversation_id, account_id, source, guid, timestamp,
           is_from_me, sort_order, body, subject
         ) VALUES (?, ?, 'imessage', 'g-search-1', '2021-06-01T12:00:00Z', 0, 0, ?, NULL)`,
      ).run(conversationId, accountId, "unique zebra pineapple vault");
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
});

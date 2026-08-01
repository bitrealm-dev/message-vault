import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";
import Database from "better-sqlite3";

import { runWithAccount } from "./accountScope";
import { createAccount, saveAccount } from "./accounts";
import { trashConversation } from "./conversationsWrite";
import { createContact } from "./contactsWrite";
import { trashHandle } from "./handlesWrite";
import { deleteAllMessagesForAccount } from "./messagesWrite";
import { VAULT_READ_ONLY_MESSAGE } from "./owner";
import { dbPath } from "./paths";
import { ensureVaultSchema } from "./vaultSchema";

describe("read-only web vault mutations", () => {
  const prevVaultDb = process.env.VAULT_DB;
  const prevVaultDataDir = process.env.VAULT_DATA_DIR;
  let tmpDir = "";
  let accountId = "";

  before(async () => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "vault-ro-"));
    process.env.VAULT_DB = path.join(tmpDir, "vault.db");
    process.env.VAULT_DATA_DIR = path.join(tmpDir, "data");
    const account = await createAccount({
      username: `ro_${Date.now()}`,
      primaryEmail: `ro_${Date.now()}@example.com`,
      firstName: "Read",
      lastName: "Only",
      phone: "+15555550100",
    });
    accountId = account.id;
    assert.equal(account.read_only, false);
    const locked = saveAccount(accountId, { read_only: true });
    assert.equal(locked.read_only, true);
  });

  after(() => {
    if (prevVaultDb === undefined) {
      delete process.env.VAULT_DB;
    } else {
      process.env.VAULT_DB = prevVaultDb;
    }
    if (prevVaultDataDir === undefined) {
      delete process.env.VAULT_DATA_DIR;
    } else {
      process.env.VAULT_DATA_DIR = prevVaultDataDir;
    }
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  function seedGroupConversation(chatId: string): number {
    const db = new Database(dbPath());
    try {
      ensureVaultSchema(db);
      const result = db
        .prepare(
          `INSERT INTO conversations (
             account_id, chat_identifier, service, conversation_type,
             group_title, exported_at, source_file
           ) VALUES (?, ?, 'iMessage', 'group', 'Friends', NULL, 't.json')`,
        )
        .run(accountId, chatId);
      return Number(result.lastInsertRowid);
    } finally {
      db.close();
    }
  }

  it("rejects ordinary writes but allows deleting all messages while locked", () => {
    runWithAccount(accountId, () => {
      assert.throws(
        () =>
          createContact({
            firstName: "Pat",
            lastName: "Lee",
            phones: ["+15555550111"],
          }),
        (err: unknown) =>
          err instanceof Error && err.message === VAULT_READ_ONLY_MESSAGE,
      );
      assert.throws(
        () => trashHandle("+15555550111"),
        (err: unknown) =>
          err instanceof Error && err.message === VAULT_READ_ONLY_MESSAGE,
      );
      const conversationId = seedGroupConversation("chat.group.locked");
      assert.throws(
        () => trashConversation(conversationId),
        (err: unknown) =>
          err instanceof Error && err.message === VAULT_READ_ONLY_MESSAGE,
      );
      const deleted = deleteAllMessagesForAccount(accountId);
      assert.ok(deleted.conversations >= 1);
    });
  });

  it("allows account settings to unlock and then accepts mutations", () => {
    const unlocked = saveAccount(accountId, { read_only: false });
    assert.equal(unlocked.read_only, false);

    runWithAccount(accountId, () => {
      trashHandle("+15555550999");
      const conversationId = seedGroupConversation("chat.group.unlocked");
      trashConversation(conversationId);
      const contact = createContact({
        firstName: "Pat",
        lastName: "Lee",
        phones: ["+15555550111"],
      });
      assert.ok(contact.id > 0);
      const deleted = deleteAllMessagesForAccount(accountId);
      assert.ok(deleted.conversations >= 0);
    });
  });
});

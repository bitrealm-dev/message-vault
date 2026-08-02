import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";
import Database from "better-sqlite3";

import { runWithAccount } from "./accountScope";
import { createAccount, saveAccount } from "./accounts";
import { listContacts } from "./contactsRead";
import { ensureUnknownContacts } from "./contactsWrite";
import { dbPath } from "./paths";
import { resetDb } from "./dbCore";
import { ensureVaultSchema } from "./vaultSchema";

const OWNER_PHONE = "+15555550100";
const GROUP_ONLY_PHONE = "+15555550111";
const DIRECT_PHONE = "+15555550222";

describe("unknown contact backfill", () => {
  const prevVaultDb = process.env.VAULT_DB;
  const prevVaultDataDir = process.env.VAULT_DATA_DIR;
  let tmpDir = "";
  let accountId = "";

  before(async () => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "vault-backfill-"));
    process.env.VAULT_DB = path.join(tmpDir, "vault.db");
    process.env.VAULT_DATA_DIR = path.join(tmpDir, "data");
    const account = await createAccount({
      username: `backfill_${Date.now()}`,
      preferredName: "Vault Owner",
      phone: OWNER_PHONE,
    });
    accountId = account.id;
    saveAccount(accountId, { read_only: false });

    const db = new Database(dbPath());
    try {
      ensureVaultSchema(db);
      const insertMsg = db.prepare(
        `INSERT INTO messages (
           conversation_id, account_id, source, guid, timestamp,
           is_from_me, sort_order, body, subject
         ) VALUES (?, ?, 'imessage', ?, ?, 0, 0, ?, NULL)`,
      );

      // Group chat whose members are the owner plus two others.
      const groupId = Number(
        db
          .prepare(
            `INSERT INTO conversations (
               account_id, chat_identifier, service, conversation_type,
               group_title, exported_at, source_file
             ) VALUES (?, 'chat-crew', 'iMessage', 'group', 'Crew', NULL, 't.json')`,
          )
          .run(accountId).lastInsertRowid,
      );
      const insertParticipant = db.prepare(
        `INSERT INTO participants (conversation_id, handle, name_hint)
         VALUES (?, ?, ?)`,
      );
      insertParticipant.run(groupId, OWNER_PHONE, "Vault Owner");
      insertParticipant.run(groupId, GROUP_ONLY_PHONE, "Group Only");
      insertParticipant.run(groupId, DIRECT_PHONE, "Direct Friend");
      insertMsg.run(groupId, accountId, "g-crew-1", "2023-06-01T10:00:00Z", "hi crew");

      // The same 1:1 handle the old backfill already covered.
      const directId = Number(
        db
          .prepare(
            `INSERT INTO conversations (
               account_id, chat_identifier, service, conversation_type,
               group_title, exported_at, source_file
             ) VALUES (?, ?, 'iMessage', 'individual', NULL, NULL, 't.json')`,
          )
          .run(accountId, DIRECT_PHONE).lastInsertRowid,
      );
      insertMsg.run(
        directId,
        accountId,
        "g-direct-1",
        "2023-06-02T10:00:00Z",
        "hi there",
      );
    } finally {
      db.close();
    }
    resetDb();
  });

  after(() => {
    if (prevVaultDb === undefined) delete process.env.VAULT_DB;
    else process.env.VAULT_DB = prevVaultDb;
    if (prevVaultDataDir === undefined) delete process.env.VAULT_DATA_DIR;
    else process.env.VAULT_DATA_DIR = prevVaultDataDir;
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it("creates contacts for group-only participants but never the owner", () => {
    runWithAccount(accountId, () => {
      assert.equal(ensureUnknownContacts(), 2);
      resetDb();

      const handles = new Set(
        listContacts("all").map((c) => c.preferredHandle ?? ""),
      );
      assert.ok(
        handles.has(GROUP_ONLY_PHONE),
        `group-only participant should get a contact: ${[...handles].join(", ")}`,
      );
      assert.ok(handles.has(DIRECT_PHONE));
      assert.ok(
        !handles.has(OWNER_PHONE),
        "the account holder must not become a contact",
      );

      // Nothing left to backfill on a second pass.
      assert.equal(ensureUnknownContacts(), 0);
    });
  });
});

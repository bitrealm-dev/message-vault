import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";
import Database from "better-sqlite3";

import { runWithAccount } from "./accountScope";
import { createAccount, saveAccount } from "./accounts";
import {
  commitContactsFromVcf,
  previewContactsFromVcf,
} from "./contactsVcfImport";
import { getContact, listContacts } from "./contactsRead";
import { dbPath } from "./paths";
import { ensureVaultSchema } from "./vaultSchema";

describe("contactsVcfImport preview/commit", () => {
  const prevVaultDb = process.env.VAULT_DB;
  const prevVaultDataDir = process.env.VAULT_DATA_DIR;
  let tmpDir = "";
  let accountId = "";
  let otherAccountId = "";

  before(async () => {
    tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "vault-vcf-"));
    process.env.VAULT_DB = path.join(tmpDir, "vault.db");
    process.env.VAULT_DATA_DIR = path.join(tmpDir, "data");

    const account = await createAccount({
      username: `vcf_${Date.now()}`,
      preferredName: "Vault Owner",
      phone: "+15555550000",
    });
    accountId = account.id;
    saveAccount(accountId, { read_only: false });

    const other = await createAccount({
      username: `vcf_other_${Date.now()}`,
      preferredName: "Other Owner",
      phone: "+15555550001",
    });
    otherAccountId = other.id;
    saveAccount(otherAccountId, { read_only: false });

    seedMessage(accountId, "+15551111111");
    seedMessage(otherAccountId, "+15552222222");
  });

  after(() => {
    if (prevVaultDb === undefined) delete process.env.VAULT_DB;
    else process.env.VAULT_DB = prevVaultDb;
    if (prevVaultDataDir === undefined) delete process.env.VAULT_DATA_DIR;
    else process.env.VAULT_DATA_DIR = prevVaultDataDir;
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  function seedMessage(acct: string, phone: string): void {
    const db = new Database(dbPath());
    try {
      ensureVaultSchema(db);
      const result = db
        .prepare(
          `INSERT INTO conversations (
             account_id, chat_identifier, service, conversation_type,
             group_title, exported_at, source_file
           ) VALUES (?, ?, 'SMS', 'individual', NULL, NULL, 't.json')`,
        )
        .run(acct, phone);
      const cid = Number(result.lastInsertRowid);
      db.prepare(
        `INSERT INTO participants (conversation_id, handle, name_hint)
         VALUES (?, ?, NULL)`,
      ).run(cid, phone);
      db.prepare(
        `INSERT INTO messages (
           conversation_id, account_id, source, guid, timestamp,
           is_from_me, sort_order, body
         ) VALUES (?, ?, 'sms', ?, '2020-01-01T00:00:00Z', 0, 0, 'hi')`,
      ).run(cid, acct, `g-${acct}-${phone}`);
    } finally {
      db.close();
    }
  }

  const vcf = `BEGIN:VCARD
VERSION:3.0
FN:Matched Person
N:Person;Matched;;;
TEL:+15551111111
CATEGORIES:Family,Friends
END:VCARD
BEGIN:VCARD
VERSION:3.0
FN:Unmatched Person
N:Person;Unmatched;;;
TEL:+15559999999
CATEGORIES:Work
END:VCARD
BEGIN:VCARD
VERSION:3.0
FN:No Phone
N:Phone;No;;;
EMAIL:nop@example.com
CATEGORIES:Kin
END:VCARD
`;

  it("previews only message-matched cards and their categories", () => {
    runWithAccount(accountId, () => {
      const preview = previewContactsFromVcf(vcf);
      assert.equal(preview.cardsTotal, 3);
      assert.equal(preview.matched, 1);
      assert.equal(preview.unmatched, 1);
      assert.equal(preview.skippedNoPhone, 1);
      assert.deepEqual(
        preview.categories.map((c) => c.source).sort(),
        ["Family", "Friends"],
      );
    });
  });

  it("does not match phones from another account", () => {
    runWithAccount(accountId, () => {
      const otherOnly = `BEGIN:VCARD
VERSION:3.0
FN:Other Account
N:Account;Other;;;
TEL:+15552222222
CATEGORIES:Secret
END:VCARD
`;
      const preview = previewContactsFromVcf(otherOnly);
      assert.equal(preview.matched, 0);
      assert.equal(preview.unmatched, 1);
      assert.equal(preview.categories.length, 0);
    });
  });

  it("commits selected mappings and is idempotent", () => {
    runWithAccount(accountId, () => {
      const mappings = [
        { source: "Family", target: "Kin", enabled: true },
        { source: "Friends", target: "Friends", enabled: false },
      ];
      const first = commitContactsFromVcf(vcf, mappings);
      assert.equal(first.created, 1);
      assert.equal(first.updated, 0);

      const contacts = listContacts("all");
      assert.equal(contacts.length, 1);
      const detail = getContact(contacts[0]!.id);
      assert.ok(detail);
      assert.equal(detail!.firstName, "Matched");
      assert.deepEqual(detail!.labels, ["Kin"]);
      assert.ok(!detail!.labels.includes("Friends"));
      assert.ok(!detail!.labels.includes("Work"));

      const second = commitContactsFromVcf(vcf, mappings);
      assert.equal(second.created, 0);
      assert.equal(second.updated, 0);
      assert.ok(second.skipped >= 1);

      const again = getContact(contacts[0]!.id);
      assert.deepEqual(again!.labels, ["Kin"]);
    });
  });
});

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, before, describe, it } from "node:test";
import Database from "better-sqlite3";

import { runWithAccount } from "./accountScope";
import { createAccount, saveAccount } from "./accounts";
import { searchVault, searchVaultByContact } from "./search";
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
      const insertContact = db.prepare(
        `INSERT INTO contacts (
           account_id, first_name, last_name, exclude, preferred_handle
         ) VALUES (?, ?, NULL, 0, ?)`,
      );
      const insertHandle = db.prepare(
        `INSERT INTO contact_handles (account_id, handle, contact_id)
         VALUES (?, ?, ?)`,
      );
      const assignContact = (
        handle: string,
        name: string,
        opts: { exclude?: boolean } = {},
      ) => {
        const contactId = Number(
          insertContact.run(accountId, name, handle).lastInsertRowid,
        );
        if (opts.exclude) {
          db.prepare(`UPDATE contacts SET exclude = 1 WHERE id = ?`).run(
            contactId,
          );
        }
        insertHandle.run(accountId, handle, contactId);
        return contactId;
      };
      const addToLabel = (contactId: number, label: string) => {
        db.prepare(
          `INSERT OR IGNORE INTO contact_labels (account_id, name) VALUES (?, ?)`,
        ).run(accountId, label);
        const labelId = Number(
          (
            db
              .prepare(
                `SELECT id FROM contact_labels WHERE account_id = ? AND name = ?`,
              )
              .get(accountId, label) as { id: number }
          ).id,
        );
        db.prepare(
          `INSERT OR IGNORE INTO contact_label_members (contact_id, label_id)
           VALUES (?, ?)`,
        ).run(contactId, labelId);
      };

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
      assignContact("+15555551001", "Active");
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
      assignContact("+15555551002", "Stale");
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
      assignContact("+15555551003", "Recent");
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

      // Two labeled contacts, one of them inactive, sharing a group chat.
      const labeledConvId = Number(
        insertConv.run(accountId, "+15555551004").lastInsertRowid,
      );
      const labeledId = assignContact("+15555551004", "Labeled");
      addToLabel(labeledId, "Family");
      insertMsg.run(
        labeledConvId,
        accountId,
        "g-labeled-1",
        "2022-03-01T12:00:00Z",
        "labeled kumquat note",
      );

      const inactiveConvId = Number(
        insertConv.run(accountId, "+15555551005").lastInsertRowid,
      );
      const inactiveId = assignContact("+15555551005", "Inactive", {
        exclude: true,
      });
      addToLabel(inactiveId, "Family");
      insertMsg.run(
        inactiveConvId,
        accountId,
        "g-inactive-1",
        "2022-03-02T12:00:00Z",
        "inactive kumquat note",
      );

      const groupConvId = Number(
        db
          .prepare(
            `INSERT INTO conversations (
               account_id, chat_identifier, service, conversation_type,
               group_title, exported_at, source_file
             ) VALUES (?, 'chat-kumquat', 'iMessage', 'group', 'Kumquat Crew', NULL, 't.json')`,
          )
          .run(accountId).lastInsertRowid,
      );
      const insertParticipant = db.prepare(
        `INSERT INTO participants (conversation_id, handle, name_hint)
         VALUES (?, ?, ?)`,
      );
      insertParticipant.run(groupConvId, "+15555551004", "Labeled");
      insertParticipant.run(groupConvId, "+15555551005", "Inactive");
      insertMsg.run(
        groupConvId,
        accountId,
        "g-group-1",
        "2022-04-01T12:00:00Z",
        "group kumquat plans",
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
      const selectedHit = result.hits.find(
        (h) => h.chatIdentifier === "+15555551002",
      );
      assert.ok(handles.includes("+15555551002"));
      assert.ok(selectedHit?.contactId != null);
      assert.ok(result.contactIds.includes(selectedHit.contactId));
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

  it("filters by an on-or-after last-contact bound", () => {
    runWithAccount(accountId, () => {
      const cutoff = new Date(Date.now() - 30 * 86_400_000)
        .toISOString()
        .slice(0, 10);
      const handles = searchVault(`last-contact:>=${cutoff}`).hits.map(
        (h) => h.chatIdentifier,
      );
      assert.ok(handles.includes("+15555551001"));
      assert.ok(handles.includes("+15555551003"));
      assert.ok(!handles.includes("+15555551002"));
    });
  });

  it("filters by a first-contact range", () => {
    runWithAccount(accountId, () => {
      const handles = searchVault(
        "first-contact:2022-02-01..2022-03-02",
      ).hits.map((h) => h.chatIdentifier);
      assert.ok(handles.includes("+15555551004"));
      assert.ok(!handles.includes("+15555551005"));
      assert.ok(!handles.includes("+15555551001"));
    });
  });

  it("within: searches a label's contacts including inactive ones", () => {
    runWithAccount(accountId, () => {
      const handles = searchVault("kumquat within:Family").hits.map(
        (h) => h.chatIdentifier,
      );
      assert.ok(handles.includes("+15555551004"));
      assert.ok(handles.includes("+15555551005"));
      assert.ok(handles.includes("chat-kumquat"));

      const other = searchVault("kumquat within:Nobody").hits;
      assert.equal(other.length, 0);
    });
  });

  it("groups results by contact, nesting shared group chats", () => {
    runWithAccount(accountId, () => {
      const result = searchVaultByContact("kumquat");
      const names = result.contacts?.map((c) => c.contact.displayName) ?? [];
      assert.deepEqual([...names].sort(), ["Inactive", "Labeled"]);
      assert.equal(result.totalContacts, 2);

      for (const hit of result.contacts ?? []) {
        const titles = hit.hits.map((h) => h.title);
        assert.ok(
          titles.includes("Kumquat Crew"),
          `${hit.contact.displayName} should include the shared group chat`,
        );
        assert.equal(hit.hits.length, 2);
        assert.equal(hit.matchCount, 2);
      }
    });
  });

  it("groups group-only matches under every participating contact", () => {
    runWithAccount(accountId, () => {
      const result = searchVaultByContact('"group kumquat plans" is:group');
      const names = (result.contacts ?? []).map((c) => c.contact.displayName);
      assert.deepEqual([...names].sort(), ["Inactive", "Labeled"]);
      for (const hit of result.contacts ?? []) {
        assert.deepEqual(
          hit.hits.map((h) => h.title),
          ["Kumquat Crew"],
        );
      }
    });
  });

  it("only heads contact groups with contacts that passed the filters", () => {
    runWithAccount(accountId, () => {
      // Both share the group chat, but only one is first contacted in March.
      const result = searchVaultByContact(
        "kumquat first-contact:2022-02-28..2022-03-02",
      );
      assert.deepEqual(
        (result.contacts ?? []).map((c) => c.contact.displayName),
        ["Labeled"],
      );
      const titles = result.contacts?.[0]?.hits.map((h) => h.title) ?? [];
      assert.ok(titles.includes("Kumquat Crew"));
    });
  });

  it("returns no contacts when nothing matches", () => {
    runWithAccount(accountId, () => {
      const result = searchVaultByContact("hapaxlegomenonxyz");
      assert.deepEqual(result.contacts, []);
      assert.equal(result.totalContacts, 0);
    });
  });
});

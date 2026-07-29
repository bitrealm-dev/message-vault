import Database from "better-sqlite3";

import { resetDb } from "./db";
import { assertVaultWritable } from "./owner";
import { dbPath } from "./paths";
import { ensureVaultSchema } from "./vaultSchema";

export type DeletedMessagesResult = {
  conversations: number;
};

/**
 * Permanently delete one account's conversations and import staging rows.
 * Contacts, labels, login details, and import token are retained.
 */
export function deleteAllMessagesForAccount(accountId: string): DeletedMessagesResult {
  assertVaultWritable();

  const db = new Database(dbPath());
  try {
    ensureVaultSchema(db);
    db.pragma("foreign_keys = ON");

    const deleteAll = db.transaction(() => {
      const conversations = db
        .prepare(`DELETE FROM conversations WHERE account_id = ?`)
        .run(accountId).changes;
      db
        .prepare(`DELETE FROM staging_conversations WHERE account_id = ?`)
        .run(accountId);
      db
        .prepare(`DELETE FROM trashed_conversations WHERE account_id = ?`)
        .run(accountId);
      db.prepare(`DELETE FROM trashed_handles WHERE account_id = ?`).run(accountId);
      return { conversations };
    });

    return deleteAll();
  } finally {
    db.close();
    resetDb();
  }
}

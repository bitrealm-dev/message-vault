import Database from "better-sqlite3";
import fs from "fs";

import { resetDb } from "./db";
import { dbPath, loadSources } from "./paths";
import { ensureVaultSchema } from "./vaultSchema";

export type DeletedMessagesResult = {
  conversations: number;
  attachments: number;
};

/**
 * Permanently delete one account's conversations and import staging rows.
 * Contacts, labels, login details, and import token are retained.
 */
export function deleteAllMessagesForAccount(accountId: string): DeletedMessagesResult {
  const sourcePaths = loadSources(accountId);
  const db = new Database(dbPath());
  try {
    ensureVaultSchema(db);
    db.pragma("foreign_keys = ON");

    const deleteAll = db.transaction(() => {
      const attachmentRow = db
        .prepare(
          `SELECT COUNT(*) AS count
           FROM attachments a
           JOIN messages m ON m.id = a.message_id
           JOIN conversations c ON c.id = m.conversation_id
           WHERE c.account_id = ?`,
        )
        .get(accountId) as { count: number };
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
      return { conversations, attachments: attachmentRow.count };
    });

    const deleted = deleteAll();
    for (const source of sourcePaths) {
      fs.rmSync(source.assetsDir, { recursive: true, force: true });
      fs.rmSync(source.assetsConvertedDir, { recursive: true, force: true });
    }
    return deleted;
  } finally {
    db.close();
    resetDb();
  }
}

import Database from "better-sqlite3";

import { currentAccountId } from "./accountScope";
import { resetDb } from "./db";
import { assertVaultWritable } from "./owner";
import { dbPath } from "./paths";

export type MessageTrashTargets = {
  handles: string[];
  conversationIds: number[];
};

export type MessageTrashWriteResult = MessageTrashTargets & {
  count: number;
};

function normalizeTargets(targets: MessageTrashTargets): MessageTrashTargets {
  return {
    handles: [
      ...new Set(targets.handles.map((handle) => handle.trim()).filter(Boolean)),
    ],
    conversationIds: [
      ...new Set(targets.conversationIds.filter((id) => Number.isFinite(id))),
    ],
  };
}

function ensureTrashTables(db: Database.Database): void {
  db.exec(`
    CREATE TABLE IF NOT EXISTS trashed_handles (
      account_id TEXT NOT NULL,
      handle TEXT NOT NULL,
      trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
      PRIMARY KEY (account_id, handle)
    );
    CREATE TABLE IF NOT EXISTS trashed_conversations (
      account_id TEXT NOT NULL,
      conversation_id INTEGER NOT NULL,
      trashed_at TEXT NOT NULL DEFAULT (datetime('now')),
      PRIMARY KEY (account_id, conversation_id)
    );
  `);
}

/**
 * Write direct-handle and group-conversation trash markers atomically.
 * Direct handles remain assigned to their contacts.
 */
export function setMessageTrashInDb(
  db: Database.Database,
  targets: MessageTrashTargets,
  trashed: boolean,
  accountId: string = currentAccountId(),
): MessageTrashWriteResult {
  const normalized = normalizeTargets(targets);
  if (normalized.handles.length + normalized.conversationIds.length === 0) {
    throw new Error("handles or conversationIds required");
  }

  ensureTrashTables(db);
  const transaction = db.transaction(() => {
    if (trashed) {
      const findGroup = db.prepare(
        `SELECT 1 AS ok FROM conversations
         WHERE id = ? AND account_id = ? AND conversation_type = 'group'`,
      );
      for (const conversationId of normalized.conversationIds) {
        if (!findGroup.get(conversationId, accountId)) {
          throw new Error(`group conversation ${conversationId} not found`);
        }
      }

      const trashHandle = db.prepare(
        `INSERT INTO trashed_handles (account_id, handle, trashed_at)
         VALUES (?, ?, datetime('now'))
         ON CONFLICT(account_id, handle) DO UPDATE SET trashed_at = excluded.trashed_at`,
      );
      const trashConversation = db.prepare(
        `INSERT INTO trashed_conversations (account_id, conversation_id, trashed_at)
         VALUES (?, ?, datetime('now'))
         ON CONFLICT(account_id, conversation_id) DO UPDATE SET trashed_at = excluded.trashed_at`,
      );
      for (const handle of normalized.handles) {
        trashHandle.run(accountId, handle);
      }
      for (const conversationId of normalized.conversationIds) {
        trashConversation.run(accountId, conversationId);
      }
      return;
    }

    const restoreHandle = db.prepare(
      `DELETE FROM trashed_handles WHERE account_id = ? AND handle = ?`,
    );
    const restoreConversation = db.prepare(
      `DELETE FROM trashed_conversations
       WHERE account_id = ? AND conversation_id = ?`,
    );
    for (const handle of normalized.handles) {
      restoreHandle.run(accountId, handle);
    }
    for (const conversationId of normalized.conversationIds) {
      restoreConversation.run(accountId, conversationId);
    }
  });
  transaction();

  return {
    ...normalized,
    count: normalized.handles.length + normalized.conversationIds.length,
  };
}

function writeMessageTrash(
  targets: MessageTrashTargets,
  trashed: boolean,
): MessageTrashWriteResult {
  assertVaultWritable();
  const accountId = currentAccountId();
  const writeDb = new Database(dbPath());
  try {
    return setMessageTrashInDb(writeDb, targets, trashed, accountId);
  } finally {
    writeDb.close();
    resetDb();
  }
}

/** Trash a mixed batch of direct and group message threads. */
export function trashMessageThreads(
  targets: MessageTrashTargets,
): MessageTrashWriteResult {
  return writeMessageTrash(targets, true);
}

/** Restore a mixed batch of direct and group message threads. */
export function restoreMessageThreads(
  targets: MessageTrashTargets,
): MessageTrashWriteResult {
  return writeMessageTrash(targets, false);
}

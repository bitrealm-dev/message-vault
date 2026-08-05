import Database from "better-sqlite3";
import { currentAccountId } from "./accountScope";
import { handleIdsForRaws, resetDb } from "./dbCore";
import { inferHandleType, normalizeHandle, type HandleType } from "./handleKind";
import { assertVaultWritable } from "./owner";
import { openWritableVaultDb } from "./vaultSchema";

/**
 * Ensure a `handles` row exists for (raw, handle_type) and return its id.
 * Matching is by (account_id, normalized, handle_type) — the handles table's
 * identity key — so a differently-formatted raw of the same handle reuses the
 * existing row.
 */
export function resolveHandleId(
  db: Database.Database,
  accountId: string,
  raw: string,
  handleType: HandleType,
): number {
  const trimmed = raw.trim();
  if (!trimmed) throw new Error("handle required");
  const normalized = normalizeHandle(trimmed, handleType);
  db.prepare(
    `INSERT OR IGNORE INTO handles (account_id, raw, normalized, handle_type, service)
     VALUES (?, ?, ?, ?, NULL)`,
  ).run(accountId, trimmed, normalized, handleType);
  const row = db
    .prepare(
      `SELECT id FROM handles
       WHERE account_id = ? AND normalized = ? AND handle_type = ?`,
    )
    .get(accountId, normalized, handleType) as { id: number } | undefined;
  if (!row) throw new Error(`failed to resolve handle ${trimmed}`);
  return row.id;
}

/** Handle id for an existing raw handle, or null when no such handle row. */
export function handleIdForRaw(
  db: Database.Database,
  accountId: string,
  raw: string,
): number | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  const type = inferHandleType(trimmed);
  const normalized = normalizeHandle(trimmed, type);
  const row = db
    .prepare(
      `SELECT id FROM handles
       WHERE account_id = ? AND normalized = ? AND handle_type = ?`,
    )
    .get(accountId, normalized, type) as { id: number } | undefined;
  return row?.id ?? null;
}

/** Remove handles from trash (e.g. after assigning to a contact). */
export function clearTrashedHandles(
  db: Database.Database,
  handles: string[],
  accountId: string = currentAccountId(),
): void {
  const ids = handleIdsForRaws(db, accountId, handles);
  if (ids.length === 0) return;
  const placeholders = ids.map(() => "?").join(",");
  db.prepare(
    `DELETE FROM trashed_handles
     WHERE account_id = ? AND handle_id IN (${placeholders})`,
  ).run(accountId, ...ids);
}

/** Upsert handles into trashed_handles (owned or unassigned). */
export function trashHandlesInDb(
  db: Database.Database,
  handles: string[],
  accountId: string = currentAccountId(),
): void {
  const trimmed = [...new Set(handles.map((h) => h.trim()).filter(Boolean))];
  if (trimmed.length === 0) return;
  const upsert = db.prepare(
    `INSERT INTO trashed_handles (account_id, handle_id, trashed_at)
     VALUES (?, ?, datetime('now'))
     ON CONFLICT(account_id, handle_id) DO UPDATE SET trashed_at = excluded.trashed_at`,
  );
  for (const handle of trimmed) {
    const handleId = resolveHandleId(
      db,
      accountId,
      handle,
      inferHandleType(handle),
    );
    upsert.run(accountId, handleId);
  }
}

/** Move a handle into Trash (may still belong to a contact). */
export function trashHandle(handle: string): void {
  assertVaultWritable();
  const accountId = currentAccountId();
  const trimmed = handle.trim();
  if (!trimmed) throw new Error("handle required");

  const writeDb = openWritableVaultDb();
  try {
    trashHandlesInDb(writeDb, [trimmed], accountId);
  } finally {
    writeDb.close();
  }
  resetDb();
}

/** Restore a handle from Trash. */
export function restoreHandle(handle: string): void {
  assertVaultWritable();
  const accountId = currentAccountId();
  const trimmed = handle.trim();
  if (!trimmed) throw new Error("handle required");

  const writeDb = openWritableVaultDb();
  try {
    const handleId = handleIdForRaw(writeDb, accountId, trimmed);
    if (handleId != null) {
      writeDb
        .prepare(`DELETE FROM trashed_handles WHERE account_id = ? AND handle_id = ?`)
        .run(accountId, handleId);
    }
  } finally {
    writeDb.close();
  }
  resetDb();
}

/**
 * Permanently remove a trashed handle: deletes its 1:1 conversation (cascades
 * messages/attachments) and removes the trash entry. Contact ownership is OK
 * (messages-only trash for a live contact).
 */
export function permanentlyDeleteHandle(handle: string): void {
  assertVaultWritable();
  const accountId = currentAccountId();
  const trimmed = handle.trim();
  if (!trimmed) throw new Error("handle required");

  const writeDb = openWritableVaultDb();
  try {
    const handleId = handleIdForRaw(writeDb, accountId, trimmed);
    if (handleId == null) {
      throw new Error("handle is not in trash");
    }
    const trashed = writeDb
      .prepare(
        `SELECT 1 AS ok FROM trashed_handles WHERE account_id = ? AND handle_id = ?`,
      )
      .get(accountId, handleId) as { ok: number } | undefined;
    if (!trashed) {
      throw new Error("handle is not in trash");
    }

    writeDb.pragma("foreign_keys = ON");
    const tx = writeDb.transaction(() => {
      writeDb
        .prepare(
          `DELETE FROM conversations
           WHERE account_id = ? AND conversation_type = 'individual' AND chat_handle_id = ?`,
        )
        .run(accountId, handleId);
      writeDb
        .prepare(`DELETE FROM trashed_handles WHERE account_id = ? AND handle_id = ?`)
        .run(accountId, handleId);
    });
    tx();
  } finally {
    writeDb.close();
  }
  resetDb();
}

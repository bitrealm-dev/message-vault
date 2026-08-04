import { getDb, resetDb } from "./dbCore";
import { openWritableVaultDb } from "./vaultSchema";

export type VaultImportListItem = {
  id: number;
  source: string;
  tool: string | null;
  mode: string;
  status: string;
  startedAt: string;
  finishedAt: string | null;
  messageCount: number;
  attachmentCount: number;
  bytesUploaded: number;
};

export type TopAttachmentItem = {
  id: number;
  originalName: string | null;
  mimeType: string | null;
  sizeBytes: number;
  conversationId: number;
  conversationTitle: string | null;
  chatIdentifier: string;
};

export type StorageUsage = {
  totalBytes: number;
  attachmentCount: number;
  topAttachments: TopAttachmentItem[];
};

function ensureReadableSchema(): void {
  // Migrate vault_imports / import_id if needed, then reopen readonly cache.
  openWritableVaultDb().close();
  resetDb();
}

export function listVaultImports(
  accountId: string,
  limit = 100,
): VaultImportListItem[] {
  ensureReadableSchema();
  const db = getDb();
  const rows = db
    .prepare(
      `SELECT id, source, tool, mode, status, started_at, finished_at,
              message_count, attachment_count, bytes_uploaded
       FROM vault_imports
       WHERE account_id = ?
       ORDER BY started_at DESC, id DESC
       LIMIT ?`,
    )
    .all(accountId, limit) as Array<{
    id: number;
    source: string;
    tool: string | null;
    mode: string;
    status: string;
    started_at: string;
    finished_at: string | null;
    message_count: number;
    attachment_count: number;
    bytes_uploaded: number;
  }>;
  return rows.map((row) => ({
    id: row.id,
    source: row.source,
    tool: row.tool,
    mode: row.mode,
    status: row.status,
    startedAt: row.started_at,
    finishedAt: row.finished_at,
    messageCount: row.message_count,
    attachmentCount: row.attachment_count,
    bytesUploaded: row.bytes_uploaded,
  }));
}

export function loadStorageUsage(
  accountId: string,
  topLimit = 20,
): StorageUsage {
  ensureReadableSchema();
  const db = getDb();
  const totals = db
    .prepare(
      `SELECT COALESCE(SUM(a.size_bytes), 0) AS total_bytes,
              COUNT(*) AS attachment_count
       FROM attachments a
       JOIN messages m ON m.id = a.message_id
       WHERE m.account_id = ?`,
    )
    .get(accountId) as { total_bytes: number; attachment_count: number };

  const top = db
    .prepare(
      `SELECT a.id,
              a.original_name,
              a.mime_type,
              COALESCE(a.size_bytes, 0) AS size_bytes,
              c.id AS conversation_id,
              c.group_title,
              c.chat_identifier
       FROM attachments a
       JOIN messages m ON m.id = a.message_id
       JOIN conversations c ON c.id = m.conversation_id
       WHERE m.account_id = ?
         AND COALESCE(a.size_bytes, 0) > 0
       ORDER BY a.size_bytes DESC, a.id DESC
       LIMIT ?`,
    )
    .all(accountId, topLimit) as Array<{
    id: number;
    original_name: string | null;
    mime_type: string | null;
    size_bytes: number;
    conversation_id: number;
    group_title: string | null;
    chat_identifier: string;
  }>;

  return {
    totalBytes: Number(totals.total_bytes) || 0,
    attachmentCount: Number(totals.attachment_count) || 0,
    topAttachments: top.map((row) => ({
      id: row.id,
      originalName: row.original_name,
      mimeType: row.mime_type,
      sizeBytes: row.size_bytes,
      conversationId: row.conversation_id,
      conversationTitle: row.group_title,
      chatIdentifier: row.chat_identifier,
    })),
  };
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value >= 10 || unit === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

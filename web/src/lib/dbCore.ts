import Database from "better-sqlite3";
import { formatPhoneDisplay } from "./phoneE164";
import { dbPath } from "./paths";

const g = globalThis as unknown as {
  __mvReadonlyDb?: Database.Database | null;
  __mvHasDuplicateOf?: boolean | null;
};

export function getDb(): Database.Database {
  if (!g.__mvReadonlyDb) {
    g.__mvReadonlyDb = new Database(dbPath(), {
      readonly: true,
      fileMustExist: true,
    });
    g.__mvReadonlyDb.pragma("foreign_keys = ON");
    g.__mvHasDuplicateOf = null;
  }
  return g.__mvReadonlyDb;
}

/** Close the cached readonly connection so the next read sees recent writes. */
export function resetDb(): void {
  if (g.__mvReadonlyDb) {
    g.__mvReadonlyDb.close();
    g.__mvReadonlyDb = null;
  }
  g.__mvHasDuplicateOf = null;
}

export function hasDuplicateOfColumn(): boolean {
  if (g.__mvHasDuplicateOf != null) return g.__mvHasDuplicateOf;
  const db = getDb();
  const row = db
    .prepare(
      `SELECT COUNT(*) AS n FROM pragma_table_info('messages') WHERE name = 'duplicate_of'`,
    )
    .get() as { n: number };
  g.__mvHasDuplicateOf = row.n > 0;
  return g.__mvHasDuplicateOf;
}

/** When no source filter is set (All combined), hide soft-deduped cross-source copies. */
export function combinedDedupeSql(source?: string | null, alias?: string): string {
  if (source || !hasDuplicateOfColumn()) return "";
  const col = alias ? `${alias}.duplicate_of` : "duplicate_of";
  return ` AND ${col} IS NULL`;
}

export function displayName(row: {
  first_name: string | null;
  last_name: string | null;
  preferred_handle: string | null;
}): string {
  const parts = [row.first_name, row.last_name]
    .map((p) => p?.trim())
    .filter(Boolean) as string[];
  if (parts.length) return parts.join(" ");
  if (row.preferred_handle?.trim()) {
    return formatPhoneDisplay(row.preferred_handle);
  }
  return "Unknown";
}

export function sortFields(row: {
  first_name: string | null;
  last_name: string | null;
  preferred_handle: string | null;
}): { sortFirst: string; sortLast: string; letter: string } {
  const first = (row.first_name || "").trim();
  const last = (row.last_name || "").trim();
  // Prefer a real name over a phone for sort keys / letter buckets.
  const sortFirst = first || last || row.preferred_handle || "Unknown";
  const sortLast = last || first || row.preferred_handle || "Unknown";
  const letterSrc = sortLast;
  const ch = letterSrc.charAt(0).toUpperCase();
  const letter = ch >= "A" && ch <= "Z" ? ch : "#";
  return { sortFirst, sortLast, letter };
}

export function hasTrashedConversationsTable(db: Database.Database): boolean {
  const row = db
    .prepare(
      `SELECT COUNT(*) AS n FROM sqlite_master
       WHERE type = 'table' AND name = 'trashed_conversations'`,
    )
    .get() as { n: number };
  return row.n > 0;
}

export function hasTrashedHandlesTable(db: Database.Database): boolean {
  const row = db
    .prepare(
      `SELECT COUNT(*) AS n FROM sqlite_master
       WHERE type = 'table' AND name = 'trashed_handles'`,
    )
    .get() as { n: number };
  return row.n > 0;
}

export function hasTrashedContactsTable(db: Database.Database): boolean {
  const row = db
    .prepare(
      `SELECT COUNT(*) AS n FROM sqlite_master
       WHERE type = 'table' AND name = 'trashed_contacts'`,
    )
    .get() as { n: number };
  return row.n > 0;
}

function looksLikePhone(value: string): boolean {
  const t = value.trim();
  if (!t) return false;
  if (t.startsWith("+") && /^[+\d\s().-]+$/.test(t)) return true;
  const digits = t.replace(/\D/g, "");
  return digits.length >= 7 && digits.length === t.replace(/[\s().+-]/g, "").length;
}

export { looksLikePhone };

/** Prefer a real display hint; ignore phones and placeholder "(Unknown)" labels. */
export function usefulNameHint(
  hint: string | null | undefined,
  handle: string | null | undefined,
): string | null {
  const t = hint?.trim() || null;
  if (!t) return null;
  if (looksLikePhone(t)) return null;
  if (handle && t.toLowerCase() === handle.toLowerCase()) return null;
  if (/^\(?unknown\)?$/i.test(t)) return null;
  return t;
}


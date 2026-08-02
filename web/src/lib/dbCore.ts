import fs from "fs";

import Database from "better-sqlite3";
import { formatPhoneDisplay } from "./phoneE164";
import { ensureDbParentDir } from "./paths";

const g = globalThis as unknown as {
  __mvReadonlyDb?: Database.Database | null;
  __mvHasDuplicateOf?: boolean | null;
};

export function getDb(): Database.Database {
  if (!g.__mvReadonlyDb) {
    const file = ensureDbParentDir();
    if (!fs.existsSync(file)) {
      throw new Error(
        `Vault database not found at ${file}. From the repo root run: ./scripts/setup-demo.sh (or create an account at /login).`,
      );
    }
    g.__mvReadonlyDb = new Database(file, {
      readonly: true,
      fileMustExist: true,
      timeout: 15000,
    });
    // Prefer WAL when the writer (serve/import) has enabled it; ignore if readonly open can't set it.
    try {
      g.__mvReadonlyDb.pragma("journal_mode = WAL");
    } catch {
      /* readonly connection may not change journal mode */
    }
    g.__mvReadonlyDb.pragma("busy_timeout = 15000");
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
  const profileCache = (globalThis as unknown as {
    __mvAccountProfileCache?: Map<string, unknown>;
  }).__mvAccountProfileCache;
  profileCache?.clear();
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

/** Join first + last into a preferred display name (null when both empty). */
export function joinPreferredName(
  firstName: string | null | undefined,
  lastName: string | null | undefined,
): string | null {
  const parts = [firstName, lastName]
    .map((p) => p?.trim())
    .filter(Boolean) as string[];
  return parts.length ? parts.join(" ") : null;
}

/** Split display name on the first space: first half / remainder as last. */
export function splitNameParts(name: string | null | undefined): {
  first: string;
  last: string;
} {
  const trimmed = (name ?? "").trim();
  if (!trimmed) return { first: "", last: "" };
  const i = trimmed.indexOf(" ");
  if (i < 0) return { first: trimmed, last: trimmed };
  const first = trimmed.slice(0, i).trim();
  const last = trimmed.slice(i + 1).trim();
  return {
    first: first || last,
    last: last || first,
  };
}

export function displayName(row: {
  preferred_name?: string | null;
  preferred_handle?: string | null;
}): string {
  const preferred = row.preferred_name?.trim();
  if (preferred) return preferred;
  if (row.preferred_handle?.trim()) {
    return formatPhoneDisplay(row.preferred_handle);
  }
  return "Unknown";
}

export function sortFields(row: {
  preferred_name?: string | null;
  preferred_handle?: string | null;
}): { sortFirst: string; sortLast: string; letter: string } {
  const preferred = (row.preferred_name || "").trim();
  const handle = row.preferred_handle?.trim() || "";
  if (preferred) {
    const { first, last } = splitNameParts(preferred);
    const sortFirst = first || preferred;
    const sortLast = last || preferred;
    const ch = sortLast.charAt(0).toUpperCase();
    const letter = ch >= "A" && ch <= "Z" ? ch : "#";
    return { sortFirst, sortLast, letter };
  }
  const fallback = handle || "Unknown";
  const ch = fallback.charAt(0).toUpperCase();
  const letter = ch >= "A" && ch <= "Z" ? ch : "#";
  return { sortFirst: fallback, sortLast: fallback, letter };
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

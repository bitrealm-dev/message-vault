import Database from "better-sqlite3";
import { currentAccountId } from "./accountScope";
import {
  serializeContactsCsv,
  type VaultContactCsvRow,
} from "./contactsCsv";
import { phoneHandlesOnly } from "./handleKind";
import { dbPath } from "./paths";

/**
 * Export the current account’s contacts as a vault-owned CSV projection
 * sourced from SQLite (not the side-effect CSV mirror or raw VCF).
 *
 * Includes: sanitized phones, names, inactive flag, and every vault label.
 * Omits: DB-only email handles and all unrelated VCF fields.
 */
export function exportContactsCsvFromDb(
  accountId = currentAccountId(),
): string {
  const db = new Database(dbPath(), { readonly: true });
  try {
    const contacts = db
      .prepare(
        `SELECT id, first_name, last_name, exclude
         FROM contacts
         WHERE account_id = ?
         ORDER BY
           CASE WHEN first_name IS NULL OR TRIM(first_name) = '' THEN 1 ELSE 0 END,
           first_name COLLATE NOCASE,
           last_name COLLATE NOCASE,
           id`,
      )
      .all(accountId) as Array<{
      id: number;
      first_name: string | null;
      last_name: string | null;
      exclude: number;
    }>;

    const handleStmt = db.prepare(
      `SELECT handle FROM contact_handles
       WHERE contact_id = ? AND account_id = ?
       ORDER BY handle`,
    );
    const labelStmt = db.prepare(
      `SELECT cl.name FROM contact_label_members clm
       JOIN contact_labels cl ON cl.id = clm.label_id
       WHERE clm.contact_id = ? AND cl.account_id = ?
       ORDER BY cl.name COLLATE NOCASE`,
    );

    const rows: VaultContactCsvRow[] = [];
    for (const c of contacts) {
      const handles = handleStmt.all(c.id, accountId) as Array<{
        handle: string;
      }>;
      const phones = phoneHandlesOnly(handles.map((h) => h.handle));
      if (phones.length === 0) continue;

      const labels = (
        labelStmt.all(c.id, accountId) as Array<{ name: string }>
      ).map((l) => l.name);

      rows.push({
        phones,
        firstName: c.first_name,
        lastName: c.last_name,
        exclude: c.exclude !== 0,
        labels,
      });
    }

    return serializeContactsCsv(rows);
  } finally {
    db.close();
  }
}

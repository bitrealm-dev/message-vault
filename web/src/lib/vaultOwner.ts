import Database from "better-sqlite3";

import { getDb, resetDb } from "./dbCore";
import { formatOwnerName, parsePhoneE164 } from "./phoneE164";
import { dbPath } from "./paths";
import { ensureVaultSchema } from "./vaultSchema";

export type VaultOwner = {
  first_name: string;
  last_name: string;
  display_name: string;
  phones: string[];
  emails: string[];
};

const g = globalThis as unknown as {
  __mvOwnerCache?: Map<string, VaultOwner>;
};

function ownerCache(): Map<string, VaultOwner> {
  if (!g.__mvOwnerCache) g.__mvOwnerCache = new Map();
  return g.__mvOwnerCache;
}

export function invalidateVaultOwnerCache(accountId?: string): void {
  const cache = g.__mvOwnerCache;
  if (!cache) return;
  if (accountId) cache.delete(accountId);
  else cache.clear();
}

function openWritableDb(): Database.Database {
  const db = new Database(dbPath());
  ensureVaultSchema(db);
  return db;
}

function readOwnerFromDb(
  db: Database.Database,
  accountId: string,
): VaultOwner {
  const row = db
    .prepare(
      `SELECT first_name, last_name, display_name
       FROM vault_owners WHERE account_id = ?`,
    )
    .get(accountId) as
    | { first_name: string; last_name: string; display_name: string }
    | undefined;

  const phones = (
    db
      .prepare(
        `SELECT phone FROM vault_owner_phones WHERE account_id = ? ORDER BY phone`,
      )
      .all(accountId) as Array<{ phone: string }>
  ).map((r) => r.phone);

  const emails = (
    db
      .prepare(
        `SELECT email FROM vault_owner_emails WHERE account_id = ? ORDER BY email`,
      )
      .all(accountId) as Array<{ email: string }>
  ).map((r) => r.email);

  const first_name = row?.first_name?.trim() || "";
  const last_name = row?.last_name?.trim() || "";
  const display_name =
    formatOwnerName(first_name, last_name) ||
    row?.display_name?.trim() ||
    "Me";

  return {
    first_name,
    last_name,
    display_name,
    phones,
    emails,
  };
}

export function createVaultOwner(
  db: Database.Database,
  accountId: string,
  owner: { first_name: string; last_name: string; phones: string[] },
): void {
  const firstName = owner.first_name.trim();
  const lastName = owner.last_name.trim();
  if (!firstName) {
    throw new Error("first name is required");
  }

  const phones = owner.phones.map((p) => parsePhoneE164(p));
  if (phones.length === 0) {
    throw new Error("at least one phone is required for importing messages");
  }

  const displayName = formatOwnerName(firstName, lastName) || firstName;

  db.prepare(
    `INSERT INTO vault_owners (account_id, first_name, last_name, display_name)
     VALUES (?, ?, ?, ?)`,
  ).run(accountId, firstName, lastName, displayName);

  const insertPhone = db.prepare(
    `INSERT INTO vault_owner_phones (account_id, phone) VALUES (?, ?)`,
  );
  for (const phone of phones) {
    insertPhone.run(accountId, phone);
  }
  invalidateVaultOwnerCache(accountId);
}

export function loadVaultOwner(accountId: string): VaultOwner {
  const cache = ownerCache();
  const cached = cache.get(accountId);
  if (cached) return cached;

  let owner: VaultOwner;
  try {
    // Prefer the shared readonly connection used by browse/read APIs.
    owner = readOwnerFromDb(getDb(), accountId);
  } catch {
    const db = openWritableDb();
    try {
      owner = readOwnerFromDb(db, accountId);
    } finally {
      db.close();
    }
  }
  cache.set(accountId, owner);
  return owner;
}

export function saveVaultOwner(
  accountId: string,
  patch: Partial<VaultOwner>,
): VaultOwner {
  invalidateVaultOwnerCache(accountId);
  const db = openWritableDb();
  try {
    const current = readOwnerFromDb(db, accountId);
    const next: VaultOwner = {
      first_name: patch.first_name?.trim() ?? current.first_name,
      last_name: patch.last_name?.trim() ?? current.last_name,
      display_name:
        formatOwnerName(
          patch.first_name?.trim() ?? current.first_name,
          patch.last_name?.trim() ?? current.last_name,
        ) || current.display_name,
      phones:
        patch.phones !== undefined
          ? patch.phones.map((p) => parsePhoneE164(p))
          : current.phones,
      emails:
        patch.emails !== undefined
          ? patch.emails.filter((e) => e.trim() !== "")
          : current.emails,
    };

    db.prepare(
      `INSERT INTO vault_owners (account_id, first_name, last_name, display_name)
       VALUES (?, ?, ?, ?)
       ON CONFLICT(account_id) DO UPDATE SET
         first_name = excluded.first_name,
         last_name = excluded.last_name,
         display_name = excluded.display_name`,
    ).run(accountId, next.first_name, next.last_name, next.display_name);

    db.prepare(`DELETE FROM vault_owner_phones WHERE account_id = ?`).run(accountId);
    const insertPhone = db.prepare(
      `INSERT INTO vault_owner_phones (account_id, phone) VALUES (?, ?)`,
    );
    for (const phone of next.phones) {
      insertPhone.run(accountId, phone);
    }

    db.prepare(`DELETE FROM vault_owner_emails WHERE account_id = ?`).run(accountId);
    const insertEmail = db.prepare(
      `INSERT INTO vault_owner_emails (account_id, email) VALUES (?, ?)`,
    );
    for (const email of next.emails) {
      insertEmail.run(accountId, email.trim());
    }

    resetDb();
    ownerCache().set(accountId, next);
    return next;
  } finally {
    db.close();
  }
}

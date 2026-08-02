import Database from "better-sqlite3";

import { getDb, joinPreferredName, resetDb } from "./dbCore";
import { parsePhoneE164 } from "./phoneE164";
import { dbPath } from "./paths";
import { ensureVaultSchema } from "./vaultSchema";

export type AccountProfile = {
  first_name: string;
  last_name: string;
  preferred_name: string | null;
  display_name: string;
  phones: string[];
  emails: string[];
};

const g = globalThis as unknown as {
  __mvAccountProfileCache?: Map<string, AccountProfile>;
};

function profileCache(): Map<string, AccountProfile> {
  if (!g.__mvAccountProfileCache) g.__mvAccountProfileCache = new Map();
  return g.__mvAccountProfileCache;
}

export function invalidateAccountProfileCache(accountId?: string): void {
  const cache = g.__mvAccountProfileCache;
  if (!cache) return;
  if (accountId) cache.delete(accountId);
  else cache.clear();
}

function openWritableDb(): Database.Database {
  const db = new Database(dbPath());
  ensureVaultSchema(db);
  return db;
}

function profileDisplayName(
  preferred_name: string | null | undefined,
  first_name: string,
  last_name: string,
): string {
  const preferred = preferred_name?.trim();
  if (preferred) return preferred;
  return joinPreferredName(first_name, last_name) || "Me";
}

function readProfileFromDb(
  db: Database.Database,
  accountId: string,
): AccountProfile {
  const row = db
    .prepare(
      `SELECT first_name, last_name, preferred_name
       FROM accounts WHERE id = ?`,
    )
    .get(accountId) as
    | {
        first_name: string;
        last_name: string;
        preferred_name: string | null;
      }
    | undefined;

  const phones = (
    db
      .prepare(
        `SELECT phone FROM account_phones WHERE account_id = ? ORDER BY phone`,
      )
      .all(accountId) as Array<{ phone: string }>
  ).map((r) => r.phone);

  const emails = (
    db
      .prepare(
        `SELECT email FROM account_emails WHERE account_id = ? ORDER BY email`,
      )
      .all(accountId) as Array<{ email: string }>
  ).map((r) => r.email);

  const first_name = row?.first_name?.trim() || "";
  const last_name = row?.last_name?.trim() || "";
  const preferred_name = row?.preferred_name?.trim() || null;

  return {
    first_name,
    last_name,
    preferred_name,
    display_name: profileDisplayName(preferred_name, first_name, last_name),
    phones,
    emails,
  };
}

/** Set name columns and phones on an existing accounts row (signup / seed). */
export function createAccountProfile(
  db: Database.Database,
  accountId: string,
  profile: { first_name: string; last_name: string; phones: string[] },
): void {
  const firstName = profile.first_name.trim();
  const lastName = profile.last_name.trim();
  if (!firstName) {
    throw new Error("first name is required");
  }

  const phones = profile.phones.map((p) => parsePhoneE164(p));
  if (phones.length === 0) {
    throw new Error("at least one phone is required for importing messages");
  }

  const preferredName = joinPreferredName(firstName, lastName);

  db.prepare(
    `UPDATE accounts
     SET first_name = ?, last_name = ?, preferred_name = ?
     WHERE id = ?`,
  ).run(firstName, lastName, preferredName, accountId);

  const insertPhone = db.prepare(
    `INSERT INTO account_phones (account_id, phone) VALUES (?, ?)`,
  );
  for (const phone of phones) {
    insertPhone.run(accountId, phone);
  }
  invalidateAccountProfileCache(accountId);
}

export function loadAccountProfile(accountId: string): AccountProfile {
  const cache = profileCache();
  const cached = cache.get(accountId);
  if (cached) return cached;

  let profile: AccountProfile;
  try {
    // Prefer the shared readonly connection used by browse/read APIs.
    profile = readProfileFromDb(getDb(), accountId);
  } catch {
    const db = openWritableDb();
    try {
      profile = readProfileFromDb(db, accountId);
    } finally {
      db.close();
    }
  }
  cache.set(accountId, profile);
  return profile;
}

export function saveAccountProfile(
  accountId: string,
  patch: Partial<
    Pick<AccountProfile, "first_name" | "last_name" | "preferred_name" | "phones">
  >,
): AccountProfile {
  invalidateAccountProfileCache(accountId);
  const db = openWritableDb();
  try {
    const current = readProfileFromDb(db, accountId);
    const first_name = patch.first_name?.trim() ?? current.first_name;
    const last_name = patch.last_name?.trim() ?? current.last_name;
    const preferred_name =
      patch.preferred_name !== undefined
        ? patch.preferred_name?.trim() || null
        : patch.first_name !== undefined || patch.last_name !== undefined
          ? joinPreferredName(first_name, last_name)
          : current.preferred_name;
    const phones =
      patch.phones !== undefined
        ? patch.phones.map((p) => parsePhoneE164(p))
        : current.phones;

    const next: AccountProfile = {
      first_name,
      last_name,
      preferred_name,
      display_name: profileDisplayName(preferred_name, first_name, last_name),
      phones,
      emails: current.emails,
    };

    db.prepare(
      `UPDATE accounts
       SET first_name = ?, last_name = ?, preferred_name = ?
       WHERE id = ?`,
    ).run(next.first_name, next.last_name, next.preferred_name, accountId);

    db.prepare(`DELETE FROM account_phones WHERE account_id = ?`).run(accountId);
    const insertPhone = db.prepare(
      `INSERT INTO account_phones (account_id, phone) VALUES (?, ?)`,
    );
    for (const phone of next.phones) {
      insertPhone.run(accountId, phone);
    }

    resetDb();
    profileCache().set(accountId, next);
    return next;
  } finally {
    db.close();
  }
}

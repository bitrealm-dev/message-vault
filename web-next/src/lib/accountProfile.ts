import Database from "better-sqlite3";

import { getDb, resetDb } from "./dbCore";
import { resolveHandleId } from "./handlesWrite";
import { parsePhoneE164 } from "./phoneE164";
import { openWritableVaultDb } from "./vaultSchema";

export type AccountProfile = {
  preferred_name: string | null;
  display_name: string;
  phones: string[];
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
  return openWritableVaultDb();
}

function profileDisplayName(preferred_name: string | null | undefined): string {
  return preferred_name?.trim() || "Me";
}

function readProfileFromDb(
  db: Database.Database,
  accountId: string,
): AccountProfile {
  const row = db
    .prepare(`SELECT preferred_name FROM accounts WHERE id = ?`)
    .get(accountId) as { preferred_name: string | null } | undefined;

  const phones = (
    db
      .prepare(
        `SELECT h.raw AS phone
         FROM account_handles ah
         JOIN handles h ON h.id = ah.handle_id
         WHERE ah.account_id = ? AND h.handle_type = 'phone'
         ORDER BY h.raw`,
      )
      .all(accountId) as Array<{ phone: string }>
  ).map((r) => r.phone);

  const preferred_name = row?.preferred_name?.trim() || null;

  return {
    preferred_name,
    display_name: profileDisplayName(preferred_name),
    phones,
  };
}

/** Set preferred name and phones on an existing accounts row (signup / seed). */
export function createAccountProfile(
  db: Database.Database,
  accountId: string,
  profile: { preferred_name: string; phones: string[] },
): void {
  const preferredName = profile.preferred_name.trim();
  if (!preferredName) {
    throw new Error("display name is required");
  }

  const phones = profile.phones.map((p) => parsePhoneE164(p));
  if (phones.length === 0) {
    throw new Error("at least one phone is required for importing messages");
  }

  db.prepare(`UPDATE accounts SET preferred_name = ? WHERE id = ?`).run(
    preferredName,
    accountId,
  );

  const insertHandle = db.prepare(
    `INSERT OR IGNORE INTO account_handles (account_id, handle_id) VALUES (?, ?)`,
  );
  for (const phone of phones) {
    const handleId = resolveHandleId(db, accountId, phone, "phone");
    insertHandle.run(accountId, handleId);
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
  patch: Partial<Pick<AccountProfile, "preferred_name" | "phones">>,
): AccountProfile {
  invalidateAccountProfileCache(accountId);
  const db = openWritableDb();
  try {
    const current = readProfileFromDb(db, accountId);
    const preferred_name =
      patch.preferred_name !== undefined
        ? patch.preferred_name?.trim() || null
        : current.preferred_name;
    const phones =
      patch.phones !== undefined
        ? patch.phones.map((p) => parsePhoneE164(p))
        : current.phones;

    const next: AccountProfile = {
      preferred_name,
      display_name: profileDisplayName(preferred_name),
      phones,
    };

    db.prepare(`UPDATE accounts SET preferred_name = ? WHERE id = ?`).run(
      next.preferred_name,
      accountId,
    );

    db.prepare(`DELETE FROM account_handles WHERE account_id = ?`).run(accountId);
    const insertHandle = db.prepare(
      `INSERT OR IGNORE INTO account_handles (account_id, handle_id) VALUES (?, ?)`,
    );
    for (const phone of next.phones) {
      const handleId = resolveHandleId(db, accountId, phone, "phone");
      insertHandle.run(accountId, handleId);
    }

    resetDb();
    profileCache().set(accountId, next);
    return next;
  } finally {
    db.close();
  }
}

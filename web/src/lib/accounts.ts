import crypto from "node:crypto";
import fs from "node:fs";

import Database from "better-sqlite3";

import { accountDataDir, ensureDbParentDir } from "./paths";
import { hashPassword, passwordsMatch, validatePasswordPlaintext } from "./password";
import { createVaultOwner } from "./vaultOwner";
import { ensureVaultSchema } from "./vaultSchema";

export const INVALID_CREDENTIALS = "Invalid username or password";

export type AccountEmail = {
  email: string;
  is_primary: boolean;
};

export type Account = {
  id: string;
  username: string;
  emails: AccountEmail[];
  read_only: boolean;
};

export type AccountSummary = {
  id: string;
  username: string;
  primaryEmail: string;
};

type AccountRow = {
  id: string;
  username: string;
  read_only: number;
};

type AccountEmailRow = {
  email: string;
  is_primary: number;
};

function normalizeEmail(email: string): string {
  return email.trim().toLowerCase();
}

export function primaryEmail(account: Account): string {
  const primary = account.emails.find((entry) => entry.is_primary);
  if (!primary?.email.trim()) {
    throw new Error("account must have a primary email");
  }
  return primary.email.trim();
}

function rowToAccount(row: AccountRow, emails: AccountEmailRow[]): Account {
  const mapped = emails.map((entry) => ({
    email: entry.email,
    is_primary: entry.is_primary === 1,
  }));

  if (mapped.length === 0) {
    throw new Error("account must have a primary email");
  }

  return {
    id: row.id,
    username: row.username,
    emails: mapped,
    read_only: row.read_only === 1,
  };
}

function openDb(): Database.Database {
  const db = new Database(ensureDbParentDir(), { timeout: 15000 });
  db.pragma("journal_mode = WAL");
  db.pragma("busy_timeout = 15000");
  db.pragma("foreign_keys = ON");
  ensureVaultSchema(db);
  return db;
}

function friendlyDbError(err: unknown): Error {
  const message = err instanceof Error ? err.message : String(err);
  if (message.includes("UNIQUE constraint failed: accounts.username")) {
    return new Error("That username is already taken.");
  }
  if (message.includes("UNIQUE constraint failed: account_emails.email")) {
    return new Error("That email is already used by another account.");
  }
  if (err instanceof Error) return err;
  return new Error(message);
}

function findAccountIdByUsername(db: Database.Database, username: string): string | null {
  const row = db
    .prepare(`SELECT id FROM accounts WHERE username = ? COLLATE NOCASE`)
    .get(username) as { id: string } | undefined;
  return row?.id ?? null;
}

function validateEmails(emails: AccountEmail[]): AccountEmail[] {
  if (emails.length === 0) {
    throw new Error("account must have a primary email");
  }

  const normalized = emails.map((entry) => ({
    email: entry.email.trim(),
    is_primary: entry.is_primary,
  }));

  if (normalized.some((entry) => !entry.email)) {
    throw new Error("email addresses cannot be empty");
  }

  const primaryCount = normalized.filter((entry) => entry.is_primary).length;
  if (primaryCount !== 1) {
    throw new Error("account must have exactly one primary email");
  }

  const seen = new Set<string>();
  for (const entry of normalized) {
    const key = normalizeEmail(entry.email);
    if (seen.has(key)) {
      throw new Error("duplicate email addresses are not allowed");
    }
    seen.add(key);
  }

  return normalized;
}

function readAccountEmails(db: Database.Database, accountId: string): AccountEmailRow[] {
  return db
    .prepare(
      `SELECT email, is_primary
       FROM account_emails
       WHERE account_id = ?
       ORDER BY is_primary DESC, email COLLATE NOCASE`,
    )
    .all(accountId) as AccountEmailRow[];
}

function writeAccountEmails(
  db: Database.Database,
  accountId: string,
  emails: AccountEmail[],
): void {
  db.prepare(`DELETE FROM account_emails WHERE account_id = ?`).run(accountId);
  const insert = db.prepare(
    `INSERT INTO account_emails (account_id, email, is_primary)
     VALUES (?, ?, ?)`,
  );
  for (const entry of emails) {
    insert.run(accountId, entry.email, entry.is_primary ? 1 : 0);
  }
}

function getAccountRow(db: Database.Database, accountId: string): AccountRow | undefined {
  return db
    .prepare(`SELECT id, username, read_only FROM accounts WHERE id = ?`)
    .get(accountId) as AccountRow | undefined;
}

const API_TOKEN_ALPHANUM =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/** Import API token: `mv-user-` + 32 letters/digits. */
export function generateApiToken(): string {
  const bytes = crypto.randomBytes(32);
  let suffix = "";
  for (const b of bytes) {
    suffix += API_TOKEN_ALPHANUM[b % API_TOKEN_ALPHANUM.length]!;
  }
  return `mv-user-${suffix}`;
}

export function hashApiToken(token: string): string {
  return crypto.createHash("sha256").update(token, "utf8").digest("hex");
}

export function accountHasApiToken(accountId: string): boolean {
  const db = openDb();
  try {
    const row = db
      .prepare(
        `SELECT COUNT(*) AS n FROM account_api_tokens WHERE account_id = ?`,
      )
      .get(accountId) as { n: number };
    return row.n > 0;
  } finally {
    db.close();
  }
}

/** Create or replace the token hash; returns plaintext once. */
export function rotateAccountApiToken(accountId: string): string {
  const db = openDb();
  try {
    const row = getAccountRow(db, accountId);
    if (!row) throw new Error("account not found");
    const token = generateApiToken();
    const tokenHash = hashApiToken(token);
    const createdAt = new Date().toISOString();
    db.prepare(
      `INSERT INTO account_api_tokens (account_id, token_hash, created_at)
       VALUES (?, ?, ?)
       ON CONFLICT(account_id) DO UPDATE SET
         token_hash = excluded.token_hash,
         created_at = excluded.created_at`,
    ).run(accountId, tokenHash, createdAt);
    return token;
  } finally {
    db.close();
  }
}

export function deleteAccountApiToken(accountId: string): void {
  const db = openDb();
  try {
    const row = getAccountRow(db, accountId);
    if (!row) throw new Error("account not found");
    db.prepare(`DELETE FROM account_api_tokens WHERE account_id = ?`).run(
      accountId,
    );
  } finally {
    db.close();
  }
}

export function listAccounts(): AccountSummary[] {
  const db = openDb();
  try {
    const rows = db
      .prepare(
        `SELECT id, username FROM accounts ORDER BY username COLLATE NOCASE`,
      )
      .all() as Array<{ id: string; username: string }>;

    return rows.map((row) => {
      const emails = readAccountEmails(db, row.id);
      const account = rowToAccount(
        { ...row, read_only: 0 },
        emails,
      );
      return {
        id: row.id,
        username: row.username,
        primaryEmail: primaryEmail(account),
      };
    });
  } finally {
    db.close();
  }
}

export function getAccount(accountId: string): Account | null {
  const db = openDb();
  try {
    const row = getAccountRow(db, accountId);
    if (!row) return null;
    const emails = readAccountEmails(db, accountId);
    return rowToAccount(row, emails);
  } finally {
    db.close();
  }
}

/** True when the account was created with no password (`password_hash` NULL). */
export function accountHasNoPassword(accountId: string): boolean {
  const db = openDb();
  try {
    const row = db
      .prepare(`SELECT password_hash FROM accounts WHERE id = ?`)
      .get(accountId) as { password_hash: string | null } | undefined;
    if (!row) return false;
    return row.password_hash == null || row.password_hash === "";
  } finally {
    db.close();
  }
}

/** Replace an account password, or pass null to enable passwordless sign-in. */
export async function setAccountPassword(
  accountId: string,
  password: string | null,
): Promise<void> {
  const passwordHash = password === null ? null : await hashPassword(password);
  const db = openDb();
  try {
    const result = db
      .prepare(`UPDATE accounts SET password_hash = ? WHERE id = ?`)
      .run(passwordHash, accountId);
    if (result.changes === 0) {
      throw new Error("account not found");
    }
  } finally {
    db.close();
  }
}

export function isUsernameTaken(username: string): boolean {
  const trimmed = username.trim();
  if (!trimmed) return false;
  const db = openDb();
  try {
    return findAccountIdByUsername(db, trimmed) != null;
  } finally {
    db.close();
  }
}

/**
 * Verify username + password. Returns the account on success, or null for any failure
 * (unknown user / wrong password) so callers can show a single error message.
 */
export async function authenticateAccount(
  username: string,
  password: string,
): Promise<Account | null> {
  const trimmed = username.trim();
  if (!trimmed) return null;

  const db = openDb();
  try {
    const row = db
      .prepare(
        `SELECT id, username, read_only, password_hash
         FROM accounts WHERE username = ? COLLATE NOCASE`,
      )
      .get(trimmed) as
      | (AccountRow & { password_hash: string | null })
      | undefined;
    if (!row) return null;

    const ok = await passwordsMatch(row.password_hash, password);
    if (!ok) return null;

    const emails = readAccountEmails(db, row.id);
    return rowToAccount(row, emails);
  } finally {
    db.close();
  }
}

export async function createAccount(input: {
  username: string;
  primaryEmail: string;
  firstName: string;
  lastName: string;
  phone: string;
  /** Plaintext password, or omit/null when creating a no-password account. */
  password?: string | null;
  noPassword?: boolean;
}): Promise<Account> {
  const username = input.username.trim();
  const email = input.primaryEmail.trim();
  const firstName = input.firstName.trim();
  const lastName = input.lastName.trim();
  if (!username) throw new Error("username is required");
  if (!email) throw new Error("primary email is required");
  if (!firstName) throw new Error("first name is required");

  // Explicit noPassword, or omitted password (tests / legacy callers) → passwordless.
  const noPassword =
    input.noPassword === true ||
    input.password === null ||
    input.password === undefined;
  let passwordHash: string | null = null;
  if (!noPassword) {
    const password = input.password ?? "";
    const pwdErr = validatePasswordPlaintext(password);
    if (pwdErr) throw new Error(pwdErr);
    passwordHash = await hashPassword(password);
  }

  const db = openDb();
  try {
    const existingId = findAccountIdByUsername(db, username);
    if (existingId) {
      throw new Error("That username is already taken.");
    }

    const id = crypto.randomUUID();
    const emails = [{ email, is_primary: true }];

    try {
      db.prepare(
        `INSERT INTO accounts (id, username, read_only, password_hash)
         VALUES (?, ?, 0, ?)`,
      ).run(id, username, passwordHash);
      writeAccountEmails(db, id, emails);
      createVaultOwner(db, id, {
        first_name: firstName,
        last_name: lastName,
        phones: [input.phone],
      });
    } catch (err) {
      throw friendlyDbError(err);
    }

    return {
      id,
      username,
      emails,
      read_only: false,
    };
  } finally {
    db.close();
  }
}

export function saveAccount(
  accountId: string,
  patch: Partial<Pick<Account, "username" | "read_only" | "emails">>,
): Account {
  const db = openDb();
  try {
    const row = getAccountRow(db, accountId);
    if (!row) {
      throw new Error("account not found");
    }

    const currentEmails = readAccountEmails(db, accountId);
    const current = rowToAccount(row, currentEmails);

    const nextEmails =
      patch.emails !== undefined ? validateEmails(patch.emails) : current.emails;
    const next: Account = {
      id: accountId,
      username: patch.username?.trim() || current.username,
      emails: nextEmails,
      read_only: patch.read_only ?? current.read_only,
    };

    if (!next.username) {
      throw new Error("username is required");
    }

    db.prepare(
      `UPDATE accounts SET username = ?, read_only = ? WHERE id = ?`,
    ).run(next.username, next.read_only ? 1 : 0, accountId);

    writeAccountEmails(db, accountId, next.emails);
    return next;
  } finally {
    db.close();
  }
}

export function deleteAccount(accountId: string): void {
  const db = openDb();
  try {
    const row = getAccountRow(db, accountId);
    if (!row) {
      throw new Error("account not found");
    }

    db.pragma("foreign_keys = ON");
    db.prepare(`DELETE FROM accounts WHERE id = ?`).run(accountId);
  } finally {
    db.close();
  }

  const accountPath = accountDataDir(accountId);
  if (fs.existsSync(accountPath)) {
    fs.rmSync(accountPath, { recursive: true, force: true });
  }
}

/** @deprecated Use getAccount(accountId) with session context. */
export function loadAccount(accountId: string): Account {
  const account = getAccount(accountId);
  if (!account) {
    throw new Error("account not found");
  }
  return account;
}

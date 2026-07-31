import crypto from "node:crypto";
import fs from "node:fs";

import Database from "better-sqlite3";

import { accountDataDir, ensureDbParentDir } from "./paths";
import { createVaultOwner } from "./vaultOwner";
import { ensureVaultSchema } from "./vaultSchema";

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
  const db = new Database(ensureDbParentDir());
  ensureVaultSchema(db);
  return db;
}

function friendlyDbError(err: unknown): Error {
  const message = err instanceof Error ? err.message : String(err);
  if (message.includes("UNIQUE constraint failed: accounts.username")) {
    return new Error("That username is already taken. Select it from the list above and click Continue.");
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

export function generateApiToken(): string {
  return `mv_${crypto.randomBytes(32).toString("base64url")}`;
}

/** Return existing import API token, or create one (backfill for older accounts). */
export function ensureAccountApiToken(accountId: string): string {
  const db = openDb();
  try {
    return ensureAccountApiTokenTx(db, accountId);
  } finally {
    db.close();
  }
}

function ensureAccountApiTokenTx(db: Database.Database, accountId: string): string {
  const row = db
    .prepare(`SELECT token FROM account_api_tokens WHERE account_id = ?`)
    .get(accountId) as { token: string } | undefined;
  if (row?.token) return row.token;

  const token = generateApiToken();
  const createdAt = new Date().toISOString();
  db.prepare(
    `INSERT INTO account_api_tokens (account_id, token, created_at) VALUES (?, ?, ?)`,
  ).run(accountId, token, createdAt);
  return token;
}

/** Replace the account's import API token; returns the new plaintext value. */
export function rotateAccountApiToken(accountId: string): string {
  const db = openDb();
  try {
    const row = getAccountRow(db, accountId);
    if (!row) throw new Error("account not found");
    const token = generateApiToken();
    const createdAt = new Date().toISOString();
    db.prepare(
      `INSERT INTO account_api_tokens (account_id, token, created_at)
       VALUES (?, ?, ?)
       ON CONFLICT(account_id) DO UPDATE SET
         token = excluded.token,
         created_at = excluded.created_at`,
    ).run(accountId, token, createdAt);
    return token;
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

export function createAccount(input: {
  username: string;
  primaryEmail: string;
  firstName: string;
  lastName: string;
  phone: string;
}): Account {
  const username = input.username.trim();
  const email = input.primaryEmail.trim();
  const firstName = input.firstName.trim();
  const lastName = input.lastName.trim();
  if (!username) throw new Error("username is required");
  if (!email) throw new Error("primary email is required");
  if (!firstName) throw new Error("first name is required");

  const db = openDb();
  try {
    const existingId = findAccountIdByUsername(db, username);
    if (existingId) {
      throw new Error(
        "That username is already taken. Select it from the list above and click Continue.",
      );
    }

    const id = crypto.randomUUID();
    const emails = [{ email, is_primary: true }];

    try {
      db.prepare(
        `INSERT INTO accounts (id, username, read_only) VALUES (?, ?, 1)`,
      ).run(id, username);
      writeAccountEmails(db, id, emails);
      createVaultOwner(db, id, {
        first_name: firstName,
        last_name: lastName,
        phones: [input.phone],
      });
      ensureAccountApiTokenTx(db, id);
    } catch (err) {
      throw friendlyDbError(err);
    }

    return {
      id,
      username,
      emails,
      read_only: true,
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

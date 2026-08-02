import Database from "better-sqlite3";

import {
  DATE_CUSTOM_KEY,
  DATE_MODE_KEY,
  isDateFormatMode,
  isTimeFormatMode,
  TIME_CUSTOM_KEY,
  TIME_MODE_KEY,
  validateDatePattern,
  validateTimePattern,
} from "./dateTimeFormat";
import {
  isBadgeVisibility,
  SHOW_CONTACT_DATE_RANGE_KEY,
  SHOW_CONTACT_INITIALS_KEY,
  SHOW_GROUP_MESSAGE_BADGE_KEY,
  SHOW_MESSAGE_BADGE_KEY,
} from "./messageBadgePrefs";
import {
  isThemeMode,
  parseThemeShare,
  THEME_MODE_KEY,
  THEME_SEEDS_KEY,
} from "./theme";
import { openWritableVaultDb } from "./vaultSchema";

export const ACCOUNT_PREF_KEYS = [
  DATE_MODE_KEY,
  DATE_CUSTOM_KEY,
  TIME_MODE_KEY,
  TIME_CUSTOM_KEY,
  SHOW_MESSAGE_BADGE_KEY,
  SHOW_GROUP_MESSAGE_BADGE_KEY,
  SHOW_CONTACT_INITIALS_KEY,
  SHOW_CONTACT_DATE_RANGE_KEY,
  THEME_MODE_KEY,
  THEME_SEEDS_KEY,
] as const;

export type AccountPrefKey = (typeof ACCOUNT_PREF_KEYS)[number];

const PREF_KEY_SET = new Set<string>(ACCOUNT_PREF_KEYS);

export class AccountPrefError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AccountPrefError";
  }
}

function openDb(): Database.Database {
  return openWritableVaultDb();
}

export function isAccountPrefKey(key: string): key is AccountPrefKey {
  return PREF_KEY_SET.has(key);
}

/** Validate a single pref value. Returns an error message or null if ok. */
export function validateAccountPref(
  key: string,
  value: string,
): string | null {
  if (!isAccountPrefKey(key)) {
    return `unknown preference key: ${key}`;
  }
  switch (key) {
    case DATE_MODE_KEY:
      return isDateFormatMode(value) ? null : "invalid date mode";
    case DATE_CUSTOM_KEY: {
      const v = validateDatePattern(value);
      return v.ok ? null : v.error;
    }
    case TIME_MODE_KEY:
      return isTimeFormatMode(value) ? null : "invalid time mode";
    case TIME_CUSTOM_KEY: {
      const v = validateTimePattern(value);
      return v.ok ? null : v.error;
    }
    case SHOW_MESSAGE_BADGE_KEY:
    case SHOW_GROUP_MESSAGE_BADGE_KEY:
    case SHOW_CONTACT_INITIALS_KEY:
    case SHOW_CONTACT_DATE_RANGE_KEY:
      return isBadgeVisibility(value) ? null : "expected on or off";
    case THEME_MODE_KEY:
      return isThemeMode(value) ? null : "invalid theme mode";
    case THEME_SEEDS_KEY:
      return parseThemeShare(value) ? null : "invalid theme seeds";
    default:
      return "unknown preference key";
  }
}

export function getAccountPrefs(accountId: string): Record<string, string> {
  const db = openDb();
  try {
    const rows = db
      .prepare(
        `SELECT key, value FROM account_prefs WHERE account_id = ?`,
      )
      .all(accountId) as { key: string; value: string }[];
    const out: Record<string, string> = {};
    for (const row of rows) {
      if (isAccountPrefKey(row.key)) {
        out[row.key] = row.value;
      }
    }
    return out;
  } finally {
    db.close();
  }
}

/**
 * Upsert validated prefs. Throws Error with a user-facing message on
 * unknown keys or invalid values.
 */
export function saveAccountPrefs(
  accountId: string,
  patch: Record<string, string>,
): Record<string, string> {
  const entries = Object.entries(patch);
  if (entries.length === 0) {
    throw new AccountPrefError("no preferences to update");
  }

  for (const [key, value] of entries) {
    if (typeof value !== "string") {
      throw new AccountPrefError(`invalid value for ${key}`);
    }
    const err = validateAccountPref(key, value);
    if (err) throw new AccountPrefError(err);
  }

  const db = openDb();
  try {
    const upsert = db.prepare(
      `INSERT INTO account_prefs (account_id, key, value)
       VALUES (?, ?, ?)
       ON CONFLICT(account_id, key) DO UPDATE SET value = excluded.value`,
    );
    const tx = db.transaction(() => {
      for (const [key, value] of entries) {
        upsert.run(accountId, key, value);
      }
    });
    tx();
  } finally {
    db.close();
  }
  return getAccountPrefs(accountId);
}

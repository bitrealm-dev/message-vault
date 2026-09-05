/**
 * Display preferences (theme, date format, badges) live in a cookie on this
 * dev-only build: the vault has no preferences route. Validation matches
 * `accountPrefs.ts`, which is not imported because it loads better-sqlite3.
 */
import { cookies } from "next/headers";

import {
  DATE_CUSTOM_KEY,
  DATE_MODE_KEY,
  isDateFormatMode,
  isTimeFormatMode,
  TIME_CUSTOM_KEY,
  TIME_MODE_KEY,
  validateDatePattern,
  validateTimePattern,
} from "@/lib/dateTimeFormat";
import {
  isBadgeVisibility,
  SHOW_CONTACT_DATE_RANGE_KEY,
  SHOW_CONTACT_INITIALS_KEY,
  SHOW_GROUP_MESSAGE_BADGE_KEY,
  SHOW_MESSAGE_BADGE_KEY,
} from "@/lib/messageBadgePrefs";
import {
  isThemeMode,
  parseThemeShare,
  THEME_MODE_KEY,
  THEME_SEEDS_KEY,
} from "@/lib/theme";

export const PREFS_COOKIE = "mv_prefs";

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

const PREF_KEY_SET = new Set<string>(ACCOUNT_PREF_KEYS);

export class AccountPrefError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "AccountPrefError";
  }
}

export function validateAccountPref(key: string, value: string): string | null {
  if (!PREF_KEY_SET.has(key)) return `unknown preference key: ${key}`;
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

export async function getAccountPrefs(): Promise<Record<string, string>> {
  const store = await cookies();
  const raw = store.get(PREFS_COOKIE)?.value;
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    const out: Record<string, string> = {};
    for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
      if (PREF_KEY_SET.has(key) && typeof value === "string") out[key] = value;
    }
    return out;
  } catch {
    return {};
  }
}

/** Validate and merge a patch into the cookie. Throws on bad keys or values. */
export async function saveAccountPrefs(
  patch: Record<string, string>,
): Promise<Record<string, string>> {
  const entries = Object.entries(patch);
  if (entries.length === 0) throw new AccountPrefError("no preferences to update");
  for (const [key, value] of entries) {
    if (typeof value !== "string") throw new AccountPrefError(`invalid value for ${key}`);
    const err = validateAccountPref(key, value);
    if (err) throw new AccountPrefError(err);
  }
  const next = { ...(await getAccountPrefs()), ...patch };
  const store = await cookies();
  store.set({
    name: PREFS_COOKIE,
    value: JSON.stringify(next),
    httpOnly: true,
    sameSite: "lax",
    path: "/",
    maxAge: 60 * 60 * 24 * 365,
  });
  return next;
}

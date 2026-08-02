import { cookies } from "next/headers";

import { ACCOUNT_COOKIE } from "@/lib/accountCookie";
import { isHankoAuth } from "@/lib/authMode";

export { ACCOUNT_COOKIE };

const COOKIE_MAX_AGE = 60 * 60 * 24 * 30;

/** Secure cookies for HTTPS / Hanko / production deployments. */
function cookieSecure(): boolean {
  const override = process.env.COOKIE_SECURE?.trim().toLowerCase();
  if (override === "true" || override === "1") return true;
  if (override === "false" || override === "0") return false;
  return process.env.NODE_ENV === "production" || isHankoAuth();
}

export function accountCookieOptions(accountId: string) {
  return {
    name: ACCOUNT_COOKIE,
    value: accountId,
    httpOnly: true,
    sameSite: "lax" as const,
    path: "/",
    maxAge: COOKIE_MAX_AGE,
    secure: cookieSecure(),
  };
}

export function clearAccountCookieOptions() {
  return {
    name: ACCOUNT_COOKIE,
    value: "",
    httpOnly: true,
    sameSite: "lax" as const,
    path: "/",
    maxAge: 0,
    secure: cookieSecure(),
  };
}

export async function getAccountIdFromCookies(): Promise<string | null> {
  const store = await cookies();
  const value = store.get(ACCOUNT_COOKIE)?.value?.trim();
  return value || null;
}

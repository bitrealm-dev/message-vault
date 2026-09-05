import {
  clearAccountCookieOptions,
  clearSessionCookieOptions,
  getSessionTokenFromCookies,
} from "@/lib/session";
import { clearMemo, vaultFetch } from "@/lib/vault/client";
import { cookies } from "next/headers";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

/** Revoke the vault session (`POST /v1/auth/logout`) and clear the cookies. */
export async function POST() {
  const token = await getSessionTokenFromCookies();
  if (token) {
    try {
      await vaultFetch("/v1/auth/logout", { method: "POST", token });
    } catch {
      // The cookie is cleared either way; a dead token is harmless.
    }
  }
  clearMemo();
  const store = await cookies();
  store.set(clearAccountCookieOptions());
  store.set(clearSessionCookieOptions());
  return NextResponse.json({ ok: true });
}

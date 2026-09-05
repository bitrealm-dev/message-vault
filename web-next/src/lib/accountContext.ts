import { NextResponse } from "next/server";

import { runWithAccountAsync } from "./accountScope";
import { getAccountIdFromCookies, getSessionTokenFromCookies } from "./session";

export const NOT_SIGNED_IN = "Not signed in";

/** The signed-in account id. Both session cookies must be present. */
export async function requireAccountId(): Promise<string> {
  const accountId = await getAccountIdFromCookies();
  const token = await getSessionTokenFromCookies();
  if (!accountId || !token) {
    throw new Error(NOT_SIGNED_IN);
  }
  return accountId;
}

export async function withAccountHandler<T>(
  fn: (accountId: string) => T | Promise<T>,
): Promise<T> {
  const accountId = await requireAccountId();
  return runWithAccountAsync(accountId, () => fn(accountId));
}

export function unauthorizedResponse(message = "Please sign in again."): NextResponse {
  return NextResponse.json({ error: message }, { status: 401 });
}

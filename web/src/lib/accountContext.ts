import { NextResponse } from "next/server";

import { runWithAccountAsync } from "./accountScope";
import { getAccountIdFromCookies } from "./session";

export async function requireAccountId(): Promise<string> {
  const accountId = await getAccountIdFromCookies();
  if (!accountId) {
    throw new Error("Not signed in");
  }
  return accountId;
}

export async function withAccountHandler<T>(
  fn: (accountId: string) => T | Promise<T>,
): Promise<T> {
  const accountId = await requireAccountId();
  return runWithAccountAsync(accountId, () => fn(accountId));
}

export function unauthorizedResponse(message = "Not signed in"): NextResponse {
  return NextResponse.json({ error: message }, { status: 401 });
}

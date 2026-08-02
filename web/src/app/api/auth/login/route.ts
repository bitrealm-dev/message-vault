import { authenticateAccount, INVALID_CREDENTIALS } from "@/lib/accounts";
import { isHankoAuth } from "@/lib/authMode";
import { accountCookieOptions } from "@/lib/session";
import { cookies } from "next/headers";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

export async function POST(req: Request) {
  if (isHankoAuth()) {
    return NextResponse.json(
      { error: "Local login is disabled when VAULT_AUTH=hanko" },
      { status: 403 },
    );
  }

  let body: Record<string, unknown>;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "invalid JSON" }, { status: 400 });
  }

  const username = typeof body.username === "string" ? body.username.trim() : "";
  const password = typeof body.password === "string" ? body.password : "";

  if (!username) {
    return NextResponse.json(
      { error: INVALID_CREDENTIALS },
      { status: 401 },
    );
  }

  // Sign-in is user ID + password only — never email.
  const account = await authenticateAccount(username, password);
  if (!account) {
    return NextResponse.json(
      { error: INVALID_CREDENTIALS },
      { status: 401 },
    );
  }

  const store = await cookies();
  store.set(accountCookieOptions(account.id));

  return NextResponse.json({
    id: account.id,
    username: account.username,
  });
}

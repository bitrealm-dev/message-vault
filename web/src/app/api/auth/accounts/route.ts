import { createAccount, isUsernameTaken, listAccounts } from "@/lib/accounts";
import { accountCookieOptions } from "@/lib/session";
import { MAX_PASSWORD_LENGTH, validatePasswordPlaintext } from "@/lib/password";
import { cookies } from "next/headers";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

export async function GET(req: Request) {
  try {
    const url = new URL(req.url);
    const check = url.searchParams.get("username");
    if (check != null) {
      const username = check.trim();
      if (!username) {
        return NextResponse.json({ taken: false });
      }
      return NextResponse.json({ taken: isUsernameTaken(username) });
    }
    return NextResponse.json({ accounts: listAccounts() });
  } catch (err) {
    const message = err instanceof Error ? err.message : "failed to list accounts";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

export async function POST(req: Request) {
  let body: Record<string, unknown>;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "invalid JSON" }, { status: 400 });
  }

  const username = typeof body.username === "string" ? body.username.trim() : "";
  const firstName =
    typeof body.firstName === "string" ? body.firstName.trim() : "";
  const lastName = typeof body.lastName === "string" ? body.lastName.trim() : "";
  const phone = typeof body.phone === "string" ? body.phone.trim() : "";
  const noPassword = body.noPassword === true;
  const password = typeof body.password === "string" ? body.password : "";
  const primaryEmail =
    typeof body.primaryEmail === "string" && body.primaryEmail.trim()
      ? body.primaryEmail.trim()
      : username
        ? `${username.toLowerCase()}@local`
        : "";

  if (!username || !firstName || !phone) {
    return NextResponse.json(
      { error: "username, firstName, and phone are required" },
      { status: 400 },
    );
  }

  if (!noPassword) {
    const pwdErr = validatePasswordPlaintext(password);
    if (pwdErr) {
      return NextResponse.json({ error: pwdErr }, { status: 400 });
    }
    if (password.length >= MAX_PASSWORD_LENGTH) {
      return NextResponse.json(
        { error: "password must be less than 100 characters" },
        { status: 400 },
      );
    }
  }

  try {
    const account = await createAccount({
      username,
      primaryEmail,
      firstName,
      lastName,
      phone,
      password: noPassword ? null : password,
      noPassword,
    });
    const store = await cookies();
    store.set(accountCookieOptions(account.id));
    return NextResponse.json({
      id: account.id,
      username: account.username,
      primaryEmail: account.emails.find((e) => e.is_primary)?.email ?? "",
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : "create failed";
    const status =
      message.includes("already taken") ||
      message.includes("already used") ||
      message.includes("E.164")
        ? 409
        : message.includes("required") ||
            message.includes("valid phone") ||
            message.includes("password")
          ? 400
          : 500;
    return NextResponse.json({ error: message }, { status });
  }
}

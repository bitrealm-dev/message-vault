import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import {
  AccountPrefError,
  getAccountPrefs,
  saveAccountPrefs,
} from "@/lib/accountPrefs";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

function authError(err: unknown): NextResponse | null {
  if (err instanceof Error && err.message === "Not signed in") {
    return unauthorizedResponse();
  }
  return null;
}

export async function GET() {
  try {
    return await withAccountHandler(async (accountId) => {
      const prefs = getAccountPrefs(accountId);
      return NextResponse.json({ prefs });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    return NextResponse.json(
      { error: "Couldn’t load appearance settings." },
      { status: 500 },
    );
  }
}

export async function PATCH(req: Request) {
  let body: Record<string, unknown>;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json(
      { error: "Couldn’t save appearance settings." },
      { status: 400 },
    );
  }

  const raw = body.prefs;
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    return NextResponse.json(
      { error: "Couldn’t save appearance settings." },
      { status: 400 },
    );
  }

  const patch: Record<string, string> = {};
  for (const [, value] of Object.entries(raw as Record<string, unknown>)) {
    if (typeof value !== "string") {
      return NextResponse.json(
        { error: "Couldn’t save appearance settings." },
        { status: 400 },
      );
    }
    patch[key] = value;
  }

  try {
    return await withAccountHandler(async (accountId) => {
      const prefs = saveAccountPrefs(accountId, patch);
      return NextResponse.json({ prefs });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    if (err instanceof AccountPrefError) {
      return NextResponse.json(
        { error: "Check your selection and try again." },
        { status: 400 },
      );
    }
    return NextResponse.json(
      { error: "Couldn’t save appearance settings." },
      { status: 500 },
    );
  }
}

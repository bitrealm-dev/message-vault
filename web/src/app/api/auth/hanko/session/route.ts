import {
  createHankoLinkedAccount,
  findAccountByHankoUserId,
} from "@/lib/accounts";
import { isHankoAuth } from "@/lib/authMode";
import { verifyHankoSessionCookie } from "@/lib/hankoSession";
import { accountNeedsOnboarding } from "@/lib/onboarding";
import { accountCookieOptions } from "@/lib/session";
import { cookies } from "next/headers";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

export async function POST() {
  if (!isHankoAuth()) {
    return NextResponse.json({ error: "Not found" }, { status: 404 });
  }

  try {
    const { hankoUserId, email } = await verifyHankoSessionCookie();
    let account = findAccountByHankoUserId(hankoUserId);
    if (!account) {
      account = createHankoLinkedAccount({ hankoUserId, email });
    }

    const store = await cookies();
    store.set(accountCookieOptions(account.id));

    return NextResponse.json({
      id: account.id,
      needsOnboarding: accountNeedsOnboarding(account.id),
    });
  } catch (err) {
    const message =
      err instanceof Error ? err.message : "Hanko session verification failed";
    const status =
      message.includes("missing") || message.includes("invalid")
        ? 401
        : message.includes("not configured")
          ? 503
          : 401;
    return NextResponse.json({ error: message }, { status });
  }
}

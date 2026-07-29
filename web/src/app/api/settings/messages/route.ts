import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { deleteAllMessagesForAccount } from "@/lib/messagesWrite";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

function authError(err: unknown): NextResponse | null {
  if (err instanceof Error && err.message === "Not signed in") {
    return unauthorizedResponse();
  }
  return null;
}

export async function DELETE() {
  try {
    return await withAccountHandler(async (accountId) => {
      const deleted = deleteAllMessagesForAccount(accountId);
      return NextResponse.json({ ok: true, ...deleted });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "delete messages failed";
    const status = message.includes("read-only") ? 403 : 500;
    return NextResponse.json({ error: message }, { status });
  }
}

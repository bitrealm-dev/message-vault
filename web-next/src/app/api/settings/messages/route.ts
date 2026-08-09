import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { deleteAllMessagesForAccount } from "@/lib/messagesWrite";
import { NextResponse } from "next/server";
import { mutationErrorStatus } from "@/lib/owner";

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
    const message =
      err instanceof Error ? err.message : "Couldn’t delete your messages.";
    return NextResponse.json(
      { error: "Couldn’t delete your messages." },
      { status: mutationErrorStatus(message, 500) },
    );
  }
}

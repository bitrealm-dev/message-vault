import { groupChatsContainingContacts } from "@/lib/db";
import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

function authError(err: unknown): NextResponse | null {
  if (err instanceof Error && err.message === "Not signed in") {
    return unauthorizedResponse();
  }
  return null;
}

/**
 * Group chats that include every contact id (AND). Extra participants
 * allowed. Computed from `GET /v1/conversations`; the `source` filter is
 * accepted but has no effect, since the list carries no import source.
 */
export async function GET(req: Request) {
  try {
    return await withAccountHandler(async () => {
      const url = new URL(req.url);
      const ids = url.searchParams
        .get("ids")
        ?.split(",")
        .map((s) => Number(s.trim()))
        .filter((n) => Number.isFinite(n) && n > 0);
      if (!ids?.length) {
        return NextResponse.json(
          { error: "ids required" },
          { status: 400 },
        );
      }
      const groupChats = await groupChatsContainingContacts(ids);
      return NextResponse.json({ groupChats });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "load failed";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

import { decodeMessageCursor } from "@/lib/messageCursor";
import {
  DEFAULT_MESSAGE_PAGE_SIZE,
  MAX_MESSAGE_PAGE_SIZE,
} from "@/lib/messagePageSize";
import {
  messagesForConversationYear,
  messagesForConversations,
  messagesPageForConversations,
} from "@/lib/messagesRead";
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

export async function GET(req: Request) {
  const url = new URL(req.url);
  const yearParam = url.searchParams.get("year");
  const year =
    yearParam != null && yearParam !== "" ? Number(yearParam) : null;
  const source = url.searchParams.get("source");
  const pageMode =
    url.searchParams.get("page") === "1" ||
    url.searchParams.get("page") === "true";
  const beforeRaw = url.searchParams.get("before");
  const limitParam = url.searchParams.get("limit");
  const limit = limitParam != null ? Number(limitParam) : DEFAULT_MESSAGE_PAGE_SIZE;
  const rawIds = url.searchParams.get("conversationIds") ?? "";
  const conversationIds = rawIds
    .split(",")
    .map((s) => Number(s.trim()))
    .filter((n) => Number.isFinite(n));

  if (!conversationIds.length) {
    return NextResponse.json(
      { error: "conversationIds required" },
      { status: 400 },
    );
  }

  if (pageMode) {
    if (year != null) {
      return NextResponse.json(
        { error: "page mode does not accept year; omit year or page" },
        { status: 400 },
      );
    }
    if (!Number.isFinite(limit) || limit < 1) {
      return NextResponse.json({ error: "invalid limit" }, { status: 400 });
    }
    let before = null;
    if (beforeRaw) {
      before = decodeMessageCursor(beforeRaw);
      if (!before) {
        return NextResponse.json({ error: "invalid before cursor" }, { status: 400 });
      }
    }
    try {
      return await withAccountHandler(async () => {
        const page = messagesPageForConversations(conversationIds, {
          source,
          before,
          limit: Math.min(limit, MAX_MESSAGE_PAGE_SIZE),
        });
        return NextResponse.json(page);
      });
    } catch (err) {
      const auth = authError(err);
      if (auth) return auth;
      const message = err instanceof Error ? err.message : "load failed";
      return NextResponse.json({ error: message }, { status: 500 });
    }
  }

  if (year != null) {
    if (!Number.isFinite(year)) {
      return NextResponse.json({ error: "invalid year" }, { status: 400 });
    }
    try {
      return await withAccountHandler(async () => {
        const messages = messagesForConversationYear(
          conversationIds,
          year,
          source,
        );
        return NextResponse.json({ messages });
      });
    } catch (err) {
      const auth = authError(err);
      if (auth) return auth;
      const message = err instanceof Error ? err.message : "load failed";
      return NextResponse.json({ error: message }, { status: 500 });
    }
  }

  try {
    return await withAccountHandler(async () => {
      const messages = messagesForConversations(conversationIds, source);
      return NextResponse.json({ messages });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "load failed";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

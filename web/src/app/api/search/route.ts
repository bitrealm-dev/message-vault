import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import {
  searchConversationMatches,
  searchVault,
  searchVaultContacts,
} from "@/lib/search";
import { parseSearchQuery } from "@/lib/searchQuery";
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
  const q = url.searchParams.get("q") ?? "";
  const limitParam = url.searchParams.get("limit");
  const offsetParam = url.searchParams.get("offset");
  const source = url.searchParams.get("source");
  const convParam = url.searchParams.get("conv");
  const limit =
    limitParam != null && limitParam !== "" ? Number(limitParam) : undefined;
  const offset =
    offsetParam != null && offsetParam !== "" ? Number(offsetParam) : undefined;

  try {
    return await withAccountHandler(async () => {
      // Per-conversation match ids for the in-thread find bar.
      if (convParam != null && convParam !== "") {
        const conversationIds = convParam.split(",").map((part) => Number(part));
        if (
          conversationIds.length === 0 ||
          conversationIds.some((id) => !Number.isInteger(id) || id <= 0)
        ) {
          return NextResponse.json(
            { error: "Invalid conversation id" },
            { status: 400 },
          );
        }
        const result = searchConversationMatches(q, conversationIds, {
          source: source?.trim() || null,
        });
        return NextResponse.json(result);
      }
      const run =
        parseSearchQuery(q).mode === "contacts"
          ? searchVaultContacts
          : searchVault;
      const result = run(q, {
        limit: Number.isFinite(limit) ? limit : undefined,
        offset: Number.isFinite(offset) ? offset : undefined,
        source: source?.trim() || null,
      });
      return NextResponse.json(result);
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "search failed";
    // FTS syntax errors surface as SQLite errors — treat as bad query.
    const status =
      message.includes("fts5") || message.includes("MATCH") ? 400 : 500;
    return NextResponse.json({ error: message }, { status });
  }
}

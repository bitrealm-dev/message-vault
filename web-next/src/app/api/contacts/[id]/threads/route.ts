import { loadContactThreadsPage } from "@/lib/db";
import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

type Params = { params: Promise<{ id: string }> };

function authError(err: unknown): NextResponse | null {
  if (err instanceof Error && err.message === "Not signed in") {
    return unauthorizedResponse();
  }
  return null;
}

export async function GET(req: Request, { params }: Params) {
  const { id: idStr } = await params;
  const id = Number(idStr);
  if (!Number.isFinite(id)) {
    return NextResponse.json({ error: "invalid id" }, { status: 400 });
  }

  try {
    return await withAccountHandler(async () => {
      const url = new URL(req.url);
      const source = url.searchParams.get("source");
      const includeTrashed =
        url.searchParams.get("trashed") === "1" ||
        url.searchParams.get("trashed") === "true";
      const page = loadContactThreadsPage(id, source, { includeTrashed });
      if (!page) {
        return NextResponse.json({ error: "not found" }, { status: 404 });
      }
      return NextResponse.json({
        contact: page.contact,
        yearly: page.yearly,
        groupChats: page.groupChats,
        messageSources: page.messageSources,
        sourceCounts: page.sourceCounts,
      });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "load failed";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

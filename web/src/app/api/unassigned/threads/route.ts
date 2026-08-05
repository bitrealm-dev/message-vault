import { unassignedThreadsBundle } from "@/lib/db";
import type { HandleType } from "@/lib/handleKind";
import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { NextResponse } from "next/server";
import { isHandleType } from "../../contacts/handles-body";

export const runtime = "nodejs";

function authError(err: unknown): NextResponse | null {
  if (err instanceof Error && err.message === "Not signed in") {
    return unauthorizedResponse();
  }
  return null;
}

export async function GET(req: Request) {
  const url = new URL(req.url);
  const handle = url.searchParams.get("handle")?.trim() ?? "";
  if (!handle) {
    return NextResponse.json({ error: "handle required" }, { status: 400 });
  }
  const source = url.searchParams.get("source");
  const includeTrashed = url.searchParams.get("trashed") === "1";
  // The list views know the handle's type; scope the conversation lookup to it
  // so the same raw under a different type can't shadow the row.
  const rawType = url.searchParams.get("handleType")?.trim();
  if (rawType && !isHandleType(rawType)) {
    return NextResponse.json({ error: "invalid handleType" }, { status: 400 });
  }
  const handleType = (rawType || null) as HandleType | null;

  try {
    return await withAccountHandler(async () => {
      const bundle = unassignedThreadsBundle(handle, source, {
        includeTrashed,
        handleType,
      });
      if (!bundle) {
        return NextResponse.json({ error: "not found" }, { status: 404 });
      }
      return NextResponse.json(bundle);
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "load failed";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

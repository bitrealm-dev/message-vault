import { listLabelMemberContactIds } from "@/lib/db";
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

/** Members of a contact label (`GET /v1/contact-groups/{id}/members`). */
export async function GET(req: Request) {
  const name = new URL(req.url).searchParams.get("name")?.trim() ?? "";
  if (!name) {
    return NextResponse.json({ error: "name required" }, { status: 400 });
  }

  try {
    return await withAccountHandler(async () => {
      const memberContactIds = await listLabelMemberContactIds(name);
      return NextResponse.json({ name, memberContactIds });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "load failed";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

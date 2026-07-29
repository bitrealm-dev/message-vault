import {
  permanentlyDeleteHandle,
  restoreHandle,
  trashHandle,
} from "@/lib/handlesWrite";
import { ensureUnknownContacts } from "@/lib/contactsWrite";
import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { NextResponse } from "next/server";
import { mutationErrorStatus } from "@/lib/owner";

export const runtime = "nodejs";

function authError(err: unknown): NextResponse | null {
  if (err instanceof Error && err.message === "Not signed in") {
    return unauthorizedResponse();
  }
  return null;
}

export async function POST(req: Request) {
  let body: { handle?: string };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "invalid json" }, { status: 400 });
  }
  const handle = body.handle?.trim() ?? "";
  if (!handle) {
    return NextResponse.json({ error: "handle required" }, { status: 400 });
  }
  try {
    return await withAccountHandler(async () => {
      trashHandle(handle);
      return NextResponse.json({ ok: true, handle });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "trash failed";
    return NextResponse.json(
      { error: message },
      { status: mutationErrorStatus(message, 400) },
    );
  }
}

export async function DELETE(req: Request) {
  let body: { handle?: string; permanent?: boolean };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "invalid json" }, { status: 400 });
  }
  const handle = body.handle?.trim() ?? "";
  if (!handle) {
    return NextResponse.json({ error: "handle required" }, { status: 400 });
  }
  try {
    return await withAccountHandler(async () => {
      if (body.permanent) {
        permanentlyDeleteHandle(handle);
        return NextResponse.json({ ok: true, handle, permanent: true });
      }
      restoreHandle(handle);
      // Promote restored unassigned handles to nameless contacts (no Unassigned UI).
      ensureUnknownContacts();
      return NextResponse.json({ ok: true, handle });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message =
      err instanceof Error
        ? err.message
        : body.permanent
          ? "delete forever failed"
          : "restore failed";
    return NextResponse.json(
      { error: message },
      { status: mutationErrorStatus(message, 400) },
    );
  }
}

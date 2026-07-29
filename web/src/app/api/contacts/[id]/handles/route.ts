import { addPhoneToContact, removePhoneFromContact } from "@/lib/contactsWrite";
import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { NextResponse } from "next/server";
import { mutationErrorStatus } from "@/lib/owner";

export const runtime = "nodejs";

type Params = { params: Promise<{ id: string }> };

function parseHandle(body: Record<string, unknown>): string {
  if (typeof body.handle === "string") return body.handle.trim();
  if (typeof body.phone === "string") return body.phone.trim();
  return "";
}

function authError(err: unknown): NextResponse | null {
  if (err instanceof Error && err.message === "Not signed in") {
    return unauthorizedResponse();
  }
  return null;
}

export async function POST(req: Request, { params }: Params) {
  const { id: idStr } = await params;
  const id = Number(idStr);
  if (!Number.isFinite(id)) {
    return NextResponse.json({ error: "invalid id" }, { status: 400 });
  }

  let body: Record<string, unknown>;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "invalid JSON" }, { status: 400 });
  }

  const handle = parseHandle(body);
  if (!handle) {
    return NextResponse.json({ error: "handle required" }, { status: 400 });
  }

  try {
    return await withAccountHandler(async () => {
      const contact = addPhoneToContact(id, handle);
      return NextResponse.json({ contact });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "update failed";
    const status = mutationErrorStatus(
      message,
      message.includes("not found")
        ? 404
        : message.includes("already belongs")
          ? 409
          : 500,
    );
    return NextResponse.json({ error: message }, { status });
  }
}

export async function DELETE(req: Request, { params }: Params) {
  const { id: idStr } = await params;
  const id = Number(idStr);
  if (!Number.isFinite(id)) {
    return NextResponse.json({ error: "invalid id" }, { status: 400 });
  }

  let body: Record<string, unknown>;
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "invalid JSON" }, { status: 400 });
  }

  const handle = parseHandle(body);
  if (!handle) {
    return NextResponse.json({ error: "handle required" }, { status: 400 });
  }

  try {
    return await withAccountHandler(async () => {
      const contact = removePhoneFromContact(id, handle);
      return NextResponse.json({ contact });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "update failed";
    const status = mutationErrorStatus(
      message,
      message.includes("not found")
        ? 404
        : message.includes("not on contact") || message.includes("cannot remove")
          ? 400
          : 500,
    );
    return NextResponse.json({ error: message }, { status });
  }
}

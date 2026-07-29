import { getContact } from "@/lib/db";
import { patchContact, type ContactPatch } from "@/lib/contactsWrite";
import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { NextResponse } from "next/server";
import { mutationErrorStatus } from "@/lib/owner";

export const runtime = "nodejs";

type Params = { params: Promise<{ id: string }> };

function authError(err: unknown): NextResponse | null {
  if (err instanceof Error && err.message === "Not signed in") {
    return unauthorizedResponse();
  }
  return null;
}

export async function GET(_req: Request, { params }: Params) {
  const { id: idStr } = await params;
  const id = Number(idStr);
  if (!Number.isFinite(id)) {
    return NextResponse.json({ error: "invalid id" }, { status: 400 });
  }

  try {
    return await withAccountHandler(async () => {
      const contact = getContact(id);
      if (!contact) {
        return NextResponse.json({ error: "not found" }, { status: 404 });
      }
      return NextResponse.json({ contact });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "load failed";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

export async function PATCH(req: Request, { params }: Params) {
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

  const patch: ContactPatch = {};
  if (typeof body.exclude === "boolean") {
    patch.exclude = body.exclude;
  }
  const labelsBody = body.labels ?? body.contactGroups;
  if (
    Array.isArray(labelsBody) &&
    labelsBody.every((t) => typeof t === "string")
  ) {
    patch.labels = labelsBody.map((t) => t.trim()).filter(Boolean);
  }
  if (body.firstName === null || typeof body.firstName === "string") {
    patch.firstName = body.firstName;
  }
  if (body.lastName === null || typeof body.lastName === "string") {
    patch.lastName = body.lastName;
  }
  if (
    Array.isArray(body.phones) &&
    body.phones.every((p) => typeof p === "string")
  ) {
    patch.phones = body.phones.map((p) => p.trim()).filter(Boolean);
  }
  if (
    patch.exclude === undefined &&
    patch.labels === undefined &&
    patch.firstName === undefined &&
    patch.lastName === undefined &&
    patch.phones === undefined
  ) {
    return NextResponse.json(
      {
        error:
          "exclude, labels, firstName, lastName, and/or phones required",
      },
      { status: 400 },
    );
  }

  try {
    return await withAccountHandler(async () => {
      const contact = patchContact(id, patch);
      return NextResponse.json({ contact });
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "update failed";
    const status = mutationErrorStatus(
      message,
      message.includes("not found") ? 404 : 500,
    );
    return NextResponse.json({ error: message }, { status });
  }
}

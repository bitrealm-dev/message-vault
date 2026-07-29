import { importContactsFromVcf } from "@/lib/contactsVcfImport";
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
  let form: FormData;
  try {
    form = await req.formData();
  } catch {
    return NextResponse.json({ error: "expected multipart form data" }, { status: 400 });
  }

  const file = form.get("file");
  if (!(file instanceof File)) {
    return NextResponse.json(
      { error: "file field required (.vcf)" },
      { status: 400 },
    );
  }

  const name = file.name.toLowerCase();
  if (name && !name.endsWith(".vcf") && !name.endsWith(".vcard")) {
    return NextResponse.json(
      { error: "file must be a .vcf or .vcard" },
      { status: 400 },
    );
  }

  const maxBytes = 8 * 1024 * 1024;
  if (file.size > maxBytes) {
    return NextResponse.json(
      { error: "VCF file too large (max 8 MB)" },
      { status: 400 },
    );
  }

  let text: string;
  try {
    text = await file.text();
  } catch {
    return NextResponse.json({ error: "failed to read file" }, { status: 400 });
  }

  if (!text.trim()) {
    return NextResponse.json({ error: "empty VCF file" }, { status: 400 });
  }

  try {
    return await withAccountHandler(async () => {
      const summary = importContactsFromVcf(text);
      return NextResponse.json(summary);
    });
  } catch (err) {
    const auth = authError(err);
    if (auth) return auth;
    const message = err instanceof Error ? err.message : "import failed";
    return NextResponse.json(
      { error: message },
      { status: mutationErrorStatus(message, 500) },
    );
  }
}

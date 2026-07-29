import { exportContactsCsvFromDb } from "@/lib/contactsCsvExport";
import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

/** GET: download vault-owned contacts CSV for the signed-in account. */
export async function GET() {
  try {
    return await withAccountHandler(async () => {
      const csv = exportContactsCsvFromDb();
      return new NextResponse(csv, {
        status: 200,
        headers: {
          "Content-Type": "text/csv; charset=utf-8",
          "Content-Disposition": 'attachment; filename="contacts.csv"',
          "Cache-Control": "no-store",
        },
      });
    });
  } catch (err) {
    if (err instanceof Error && err.message === "Not signed in") {
      return unauthorizedResponse();
    }
    const message = err instanceof Error ? err.message : "export failed";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}

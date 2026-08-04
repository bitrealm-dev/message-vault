import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { listVaultImports } from "@/lib/storageStats";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

export async function GET() {
  try {
    return await withAccountHandler(async (accountId) => {
      const imports = listVaultImports(accountId);
      return NextResponse.json({ imports });
    });
  } catch (err) {
    if (err instanceof Error && err.message === "Not signed in") {
      return unauthorizedResponse();
    }
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Couldn’t load imports." },
      { status: 500 },
    );
  }
}

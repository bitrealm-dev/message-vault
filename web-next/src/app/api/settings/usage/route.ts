import {
  unauthorizedResponse,
  withAccountHandler,
} from "@/lib/accountContext";
import { loadStorageUsage } from "@/lib/storageStats";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

export async function GET() {
  try {
    return await withAccountHandler(async (accountId) => {
      const usage = loadStorageUsage(accountId);
      return NextResponse.json(usage);
    });
  } catch (err) {
    if (err instanceof Error && err.message === "Not signed in") {
      return unauthorizedResponse();
    }
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Couldn’t load usage." },
      { status: 500 },
    );
  }
}

import { writesNotAvailable } from "@/lib/vault/writes";
import { NextResponse } from "next/server";

export const dynamic = "force-dynamic";

/**
 * Demo reset is a vault CLI command (`reset-demo`), never an HTTP route, so
 * the title menu never offers it here.
 */
export async function GET() {
  return NextResponse.json({ available: false, hint: null });
}

export async function POST() {
  return writesNotAvailable();
}

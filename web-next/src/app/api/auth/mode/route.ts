import { NextResponse } from "next/server";

export const runtime = "nodejs";

/**
 * Public: which auth UI the client should use. The vault has one login
 * route, so this is always local; the Hanko path is no longer wired.
 */
export async function GET() {
  return NextResponse.json({ authMode: "local", hankoApiUrl: "" });
}

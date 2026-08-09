import { getAuthMode, getHankoApiUrl } from "@/lib/authMode";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

/** Public: which auth UI the client should use (logout / login branches). */
export async function GET() {
  return NextResponse.json({
    authMode: getAuthMode(),
    hankoApiUrl: getHankoApiUrl(),
  });
}

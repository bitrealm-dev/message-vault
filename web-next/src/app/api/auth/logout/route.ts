import { clearAccountCookieOptions } from "@/lib/session";
import { cookies } from "next/headers";
import { NextResponse } from "next/server";

export const runtime = "nodejs";

export async function POST() {
  const store = await cookies();
  store.set(clearAccountCookieOptions());
  return NextResponse.json({ ok: true });
}

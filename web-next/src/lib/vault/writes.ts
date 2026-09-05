/**
 * Writes are not mapped onto `/v1` yet: web-next reads the vault through the
 * API and every route handler that would change data answers 501 instead.
 * The SQL that used to serve those handlers is still in the tree; it just
 * has no caller.
 */
import { NextResponse } from "next/server";

export const WRITES_UNAVAILABLE =
  "Not available: web-next reads the vault through /v1 and has not mapped writes yet.";

/** False until the write mapping lands. Read at call time, never inlined. */
export function writesAvailable(): boolean {
  return false;
}

export function writesNotAvailable(): NextResponse {
  return NextResponse.json({ error: WRITES_UNAVAILABLE }, { status: 501 });
}

/** For reads whose data the API does not expose at all. */
export function noRoute(what: string): NextResponse {
  return NextResponse.json(
    { error: `No /v1 route for ${what}.` },
    { status: 501 },
  );
}

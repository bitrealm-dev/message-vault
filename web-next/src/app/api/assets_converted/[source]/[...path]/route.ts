import { noRoute } from "@/lib/vault/writes";

export const runtime = "nodejs";

/**
 * Transcoded media (`assets_converted/`) has no `/v1` route. Attachments
 * use the raw bytes from `/api/assets/…` instead.
 */
export async function GET() {
  return noRoute("transcoded media; the raw file is at /api/assets");
}

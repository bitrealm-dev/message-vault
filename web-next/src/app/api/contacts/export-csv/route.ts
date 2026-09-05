import { noRoute } from "@/lib/vault/writes";

export const runtime = "nodejs";

/** The vault has no contacts CSV export route. */
export async function GET() {
  return noRoute("a contacts CSV export");
}

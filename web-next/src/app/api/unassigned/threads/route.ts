import { noRoute } from "@/lib/vault/writes";

export const runtime = "nodejs";

/**
 * "Unassigned handles" no longer exist: every handle an import meets becomes
 * a contact, and `/unassigned` redirects to `/all`.
 */
export async function GET() {
  return noRoute("unassigned handles; every handle is a contact now");
}

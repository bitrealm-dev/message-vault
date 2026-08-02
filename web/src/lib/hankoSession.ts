import { createRemoteJWKSet, jwtVerify } from "jose";
import { cookies } from "next/headers";

import { getHankoApiUrl } from "@/lib/authMode";

const HANKO_COOKIE = "hanko";

export type VerifiedHankoSession = {
  hankoUserId: string;
  email: string | null;
};

function jwksFor(apiUrl: string) {
  return createRemoteJWKSet(new URL(`${apiUrl}/.well-known/jwks.json`));
}

/**
 * Verify the Hanko session cookie (`hanko`) against the project's JWKS.
 */
export async function verifyHankoSessionCookie(): Promise<VerifiedHankoSession> {
  const apiUrl = getHankoApiUrl();
  if (!apiUrl) {
    throw new Error("HANKO_API_URL is not configured");
  }

  const store = await cookies();
  const token = store.get(HANKO_COOKIE)?.value?.trim();
  if (!token) {
    throw new Error("missing Hanko session");
  }

  const { payload } = await jwtVerify(token, jwksFor(apiUrl));
  const hankoUserId =
    typeof payload.sub === "string" ? payload.sub.trim() : "";
  if (!hankoUserId) {
    throw new Error("invalid Hanko session");
  }

  const emailClaim = payload.email;
  const email =
    typeof emailClaim === "string" && emailClaim.trim()
      ? emailClaim.trim().toLowerCase()
      : null;

  return { hankoUserId, email };
}

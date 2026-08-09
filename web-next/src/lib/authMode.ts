export type AuthMode = "local" | "hanko";

/** `VAULT_AUTH=local` (default) or `hanko`. */
export function getAuthMode(): AuthMode {
  const raw = process.env.VAULT_AUTH?.trim().toLowerCase();
  return raw === "hanko" ? "hanko" : "local";
}

export function isHankoAuth(): boolean {
  return getAuthMode() === "hanko";
}

/**
 * Hanko API base URL (no trailing slash).
 * Accepts `HANKO_API_URL` or `NEXT_PUBLIC_HANKO_API_URL`.
 */
export function getHankoApiUrl(): string {
  const raw = (
    process.env.HANKO_API_URL ||
    process.env.NEXT_PUBLIC_HANKO_API_URL ||
    ""
  ).trim();
  return raw.replace(/\/+$/, "");
}

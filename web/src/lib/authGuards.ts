export type AuthMode = "hanko" | "local";

export interface ParsedPersistedAuth {
  serverUrl: string;
  token: string;
  accountId: string;
  needsOnboarding: boolean;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** True when the value is one of the two login modes the app supports. */
export function isAuthMode(value: unknown): value is AuthMode {
  return value === "hanko" || value === "local";
}

/**
 * Read a saved login session from JSON.
 * Returns null when the text is not valid JSON or required fields are missing.
 */
export function parsePersistedAuth(raw: string): ParsedPersistedAuth | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }

  if (!isRecord(parsed)) return null;

  if (typeof parsed.serverUrl !== "string") return null;
  if (typeof parsed.token !== "string" || !parsed.token) return null;
  if (typeof parsed.accountId !== "string" || !parsed.accountId) return null;

  return {
    serverUrl: parsed.serverUrl,
    token: parsed.token,
    accountId: parsed.accountId,
    needsOnboarding: parsed.needsOnboarding === true,
  };
}

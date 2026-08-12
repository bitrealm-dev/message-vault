export type AuthMode = "hanko" | "local";

interface PersistedAuthFields {
  serverUrl?: unknown;
  token?: unknown;
  accountId?: unknown;
  needsOnboarding?: unknown;
}

export interface ParsedPersistedAuth {
  serverUrl: string;
  token: string;
  accountId: string;
  needsOnboarding: boolean;
}

export function isAuthMode(value: unknown): value is AuthMode {
  return value === "hanko" || value === "local";
}

/** Parse persisted auth JSON; returns null when required fields are missing or invalid. */
export function parsePersistedAuth(raw: string): ParsedPersistedAuth | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }

  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    return null;
  }

  const data = parsed as PersistedAuthFields;

  if (typeof data.serverUrl !== "string") return null;
  if (typeof data.token !== "string" || !data.token) return null;
  if (typeof data.accountId !== "string" || !data.accountId) return null;

  return {
    serverUrl: data.serverUrl,
    token: data.token,
    accountId: data.accountId,
    needsOnboarding: data.needsOnboarding === true,
  };
}

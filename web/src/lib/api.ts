import { buildAssetPath } from "./assetUrl";

let baseUrl = "";
let authToken: string | null = null;

/** Set the vault server URL. An empty string means "same host as this page". */
export function setBaseUrl(url: string) {
  baseUrl = url.replace(/\/+$/, "");
}

/** Store the session token used on later API calls. Pass null to log out. */
export function setToken(token: string | null) {
  authToken = token;
}

/** Current session token on the API client, or null when signed out. */
export function getToken(): string | null {
  return authToken;
}

export function getBaseUrl(): string {
  return baseUrl;
}

/** An error response from the vault: its own message, plus the HTTP status. */
export class VaultApiError extends Error {
  readonly status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "VaultApiError";
    this.status = status;
  }
}

/**
 * Human-readable message for a failed response.
 *
 * The vault answers `{"ok":false,"error":"..."}`, and that sentence is what a
 * user should read — not the status code and not the envelope around it.
 * Anything else (a proxy's HTML error page, an empty body) falls back to the
 * raw text, then to a generic sentence.
 */
export function errorMessageFromBody(status: number, text: string): string {
  const trimmed = text.trim();
  if (!trimmed) return `Request failed (${status})`;

  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (parsed && typeof parsed === "object" && "error" in parsed) {
      const { error } = parsed as { error: unknown };
      if (typeof error === "string" && error.trim()) return error.trim();
    }
  } catch {
    // Not JSON — the raw text is the best available message.
  }
  return trimmed;
}

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
  signal?: AbortSignal,
): Promise<T> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (authToken) {
    headers.Authorization = `Bearer ${authToken}`;
  }

  const res = await fetch(`${baseUrl}${path}`, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
    signal,
  });

  if (!res.ok) {
    const text = await res.text();
    throw new VaultApiError(res.status, errorMessageFromBody(res.status, text));
  }

  return res.json() as Promise<T>;
}

/**
 * Download an attachment by its content hash and return a temporary blob URL.
 * The caller must call `URL.revokeObjectURL` when the URL is no longer needed.
 */
export async function fetchAssetObjectUrl(
  sha256: string,
  source: string,
  signal?: AbortSignal,
): Promise<string> {
  const path = buildAssetPath(sha256, source);
  const headers: Record<string, string> = {};
  if (authToken) {
    headers.Authorization = `Bearer ${authToken}`;
  }
  const res = await fetch(`${baseUrl}${path}`, { method: "GET", headers, signal });
  if (!res.ok) {
    const text = await res.text();
    throw new VaultApiError(res.status, errorMessageFromBody(res.status, text));
  }
  const blob = await res.blob();
  return URL.createObjectURL(blob);
}

export type ApiRequestOptions = {
  signal?: AbortSignal;
};

export const apiClient = {
  get<T>(path: string, opts?: ApiRequestOptions): Promise<T> {
    return request<T>("GET", path, undefined, opts?.signal);
  },
  post<T>(path: string, body?: unknown, opts?: ApiRequestOptions): Promise<T> {
    return request<T>("POST", path, body, opts?.signal);
  },
  patch<T>(path: string, body?: unknown, opts?: ApiRequestOptions): Promise<T> {
    return request<T>("PATCH", path, body, opts?.signal);
  },
  delete<T>(path: string, body?: unknown, opts?: ApiRequestOptions): Promise<T> {
    return request<T>("DELETE", path, body, opts?.signal);
  },
};

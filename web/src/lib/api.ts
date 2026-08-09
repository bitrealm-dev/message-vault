import { buildAssetPath } from "./assetUrl";

let baseUrl = "";
let authToken: string | null = null;

export function setBaseUrl(url: string) {
  // Empty string = same-origin (Vite proxy or vault-hosted static UI).
  baseUrl = url.replace(/\/+$/, "");
}

export function setToken(token: string | null) {
  authToken = token;
}

export function getBaseUrl(): string {
  return baseUrl;
}

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
  signal?: AbortSignal,
): Promise<T> {
  // baseUrl "" is valid (same-origin). Absolute URLs required for remote vaults.
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };
  if (authToken) {
    headers["Authorization"] = `Bearer ${authToken}`;
  }

  const res = await fetch(`${baseUrl}${path}`, {
    method,
    headers,
    body: body ? JSON.stringify(body) : undefined,
    signal,
  });

  if (!res.ok) {
    const text = await res.text();
    throw new Error(`${res.status}: ${text}`);
  }

  return res.json() as Promise<T>;
}

/**
 * Fetch a content-addressed asset with Bearer auth and return an object URL.
 * Caller must revoke the URL when done (`URL.revokeObjectURL`).
 */
export async function fetchAssetObjectUrl(
  sha256: string,
  source: string,
  signal?: AbortSignal,
): Promise<string> {
  const path = buildAssetPath(sha256, source);
  const headers: Record<string, string> = {};
  if (authToken) {
    headers["Authorization"] = `Bearer ${authToken}`;
  }
  const res = await fetch(`${baseUrl}${path}`, { method: "GET", headers, signal });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`${res.status}: ${text}`);
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
};

/** How long to wait after failure attempt `failureIndex` (0-based) before the next probe. */
export const HEALTH_BACKOFF_CAP_MS = 30_000;
export const HEALTH_SUCCESS_RECHECK_MS = 30_000;
export const HEALTH_URL_DEBOUNCE_MS = 400;

/** Progressive backoff: 1s → 2s → 4s → … capped at 30s. */
export function healthBackoffMs(failureIndex: number): number {
  const n = Math.max(0, Math.floor(failureIndex));
  const delay = 1000 * 2 ** n;
  return Math.min(delay, HEALTH_BACKOFF_CAP_MS);
}

export type VaultHealthStatus = "unknown" | "ok" | "fail";

function trimBaseUrl(baseUrl: string): string {
  return baseUrl.trim().replace(/\/+$/, "");
}

/**
 * Probe vault liveness via GET /health (plain text, not JSON).
 * Returns true only when the response is OK.
 */
export async function checkVaultHealth(baseUrl: string, signal?: AbortSignal): Promise<boolean> {
  const base = trimBaseUrl(baseUrl);
  if (!base) return false;
  try {
    const res = await fetch(`${base}/health`, {
      method: "GET",
      signal,
      cache: "no-store",
    });
    return res.ok;
  } catch {
    return false;
  }
}

export function healthStatusLabel(status: VaultHealthStatus): string {
  switch (status) {
    case "ok":
      return "Server reachable";
    case "fail":
      return "Server unreachable";
    default:
      return "Server status unknown";
  }
}

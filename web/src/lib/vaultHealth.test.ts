/** @vitest-environment jsdom */

import { afterEach, describe, expect, it, vi } from "vitest";
import {
  checkVaultHealth,
  HEALTH_BACKOFF_CAP_MS,
  healthBackoffMs,
  healthStatusLabel,
} from "./vaultHealth";

describe("healthBackoffMs", () => {
  it("grows then caps at 30s", () => {
    expect(healthBackoffMs(0)).toBe(1000);
    expect(healthBackoffMs(1)).toBe(2000);
    expect(healthBackoffMs(2)).toBe(4000);
    expect(healthBackoffMs(3)).toBe(8000);
    expect(healthBackoffMs(4)).toBe(16000);
    expect(healthBackoffMs(5)).toBe(HEALTH_BACKOFF_CAP_MS);
    expect(healthBackoffMs(10)).toBe(HEALTH_BACKOFF_CAP_MS);
  });

  it("is monotonic for successive failure indexes", () => {
    for (let i = 0; i < 8; i++) {
      expect(healthBackoffMs(i + 1)).toBeGreaterThanOrEqual(healthBackoffMs(i));
    }
  });
});

describe("healthStatusLabel", () => {
  it("names each status", () => {
    expect(healthStatusLabel("unknown")).toBe("Server status unknown");
    expect(healthStatusLabel("ok")).toBe("Server reachable");
    expect(healthStatusLabel("fail")).toBe("Server unreachable");
  });
});

describe("checkVaultHealth", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("returns true when /health is ok", async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true });
    vi.stubGlobal("fetch", fetchMock);

    await expect(checkVaultHealth("http://127.0.0.1:8080/")).resolves.toBe(true);
    expect(fetchMock).toHaveBeenCalledWith("http://127.0.0.1:8080/health", {
      method: "GET",
      signal: undefined,
      cache: "no-store",
    });
  });

  it("returns false when /health is not ok", async () => {
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: false, status: 503 }));
    await expect(checkVaultHealth("http://127.0.0.1:8080")).resolves.toBe(false);
  });

  it("returns false on network error", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("Failed to fetch")));
    await expect(checkVaultHealth("http://127.0.0.1:8080")).resolves.toBe(false);
  });

  it("returns false for a blank URL without calling fetch", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    await expect(checkVaultHealth("   ")).resolves.toBe(false);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("returns false when aborted", async () => {
    const err = new DOMException("Aborted", "AbortError");
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(err));
    const ac = new AbortController();
    ac.abort();
    await expect(checkVaultHealth("http://127.0.0.1:8080", ac.signal)).resolves.toBe(false);
  });
});

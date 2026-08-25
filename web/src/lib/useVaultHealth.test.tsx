/** @vitest-environment jsdom */

import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useVaultHealth } from "./useVaultHealth";
import { HEALTH_URL_DEBOUNCE_MS } from "./vaultHealth";

describe("useVaultHealth", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("does not fetch when probing is disabled", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    const { result } = renderHook(() => useVaultHealth(null));
    expect(result.current).toBe("unknown");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(HEALTH_URL_DEBOUNCE_MS + 50);
    });

    expect(fetchMock).not.toHaveBeenCalled();
    expect(result.current).toBe("unknown");
  });

  it("probes this origin after debounce when the URL is blank", async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: true });
    vi.stubGlobal("fetch", fetchMock);

    const { result } = renderHook(() => useVaultHealth(""));
    expect(result.current).toBe("unknown");

    await waitFor(() => expect(result.current).toBe("ok"), { timeout: 2000 });
    expect(fetchMock).toHaveBeenCalledWith(
      "/health",
      expect.objectContaining({ method: "GET", cache: "no-store" }),
    );
  });

  it("marks fail for a non-http URL without calling fetch", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    const { result } = renderHook(() => useVaultHealth("not-a-url"));
    await waitFor(() => expect(result.current).toBe("fail"), { timeout: 2000 });
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

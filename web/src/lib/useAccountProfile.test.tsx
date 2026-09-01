/** @vitest-environment jsdom */

import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AccountProfile } from "./account";
import {
  clearAccountProfile,
  loadAccountProfile,
  setAccountProfile,
  useAccountProfile,
} from "./useAccountProfile";
import { getAccountProfile } from "./vaultApi";

vi.mock("./vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./vaultApi")>()),
  getAccountProfile: vi.fn(),
}));

const get = vi.mocked(getAccountProfile);

function profile(name: string): AccountProfile {
  return {
    account_id: "a1",
    username: "demo",
    preferred_name: name,
    phones: [],
    emails: [],
    is_demo: false,
    is_admin: false,
    can_import: true,
    can_export: true,
    can_delete: true,
  };
}

describe("account profile store", () => {
  beforeEach(() => {
    get.mockReset();
    clearAccountProfile();
  });

  afterEach(() => {
    cleanup();
  });

  it("issues one request for concurrently mounted callers", async () => {
    get.mockResolvedValue(profile("Ada"));

    const a = renderHook(() => useAccountProfile());
    const b = renderHook(() => useAccountProfile());
    const c = renderHook(() => useAccountProfile());

    await waitFor(() => expect(a.result.current.profile?.preferred_name).toBe("Ada"));

    expect(get).toHaveBeenCalledTimes(1);
    expect(b.result.current.profile?.preferred_name).toBe("Ada");
    expect(c.result.current.profile?.preferred_name).toBe("Ada");
  });

  it("serves later callers from cache without another request", async () => {
    get.mockResolvedValue(profile("Ada"));

    const first = renderHook(() => useAccountProfile());
    await waitFor(() => expect(first.result.current.loading).toBe(false));
    expect(get).toHaveBeenCalledTimes(1);

    const second = renderHook(() => useAccountProfile());
    await waitFor(() => expect(second.result.current.profile?.preferred_name).toBe("Ada"));
    expect(get).toHaveBeenCalledTimes(1);
  });

  it("pushes a setProfile update to every mounted caller", async () => {
    get.mockResolvedValue(profile("Ada"));

    const a = renderHook(() => useAccountProfile());
    const b = renderHook(() => useAccountProfile());
    await waitFor(() => expect(a.result.current.profile?.preferred_name).toBe("Ada"));

    setAccountProfile(profile("Grace"));

    await waitFor(() => expect(b.result.current.profile?.preferred_name).toBe("Grace"));
    expect(a.result.current.profile?.preferred_name).toBe("Grace");
  });

  it("starts out loading so a guard cannot act on a not-yet-fetched profile", () => {
    get.mockReturnValue(new Promise(() => {}));
    const { result } = renderHook(() => useAccountProfile());
    expect(result.current.loading).toBe(true);
    expect(result.current.profile).toBeNull();
  });

  it("refetches after the cache is cleared for a new session", async () => {
    get.mockResolvedValue(profile("Ada"));
    await loadAccountProfile();
    expect(get).toHaveBeenCalledTimes(1);

    clearAccountProfile();
    get.mockResolvedValue(profile("Grace"));
    const reloaded = await loadAccountProfile();

    expect(get).toHaveBeenCalledTimes(2);
    expect(reloaded?.preferred_name).toBe("Grace");
  });

  it("reports a failed request as an error rather than throwing", async () => {
    get.mockRejectedValue(new Error("no network"));

    const { result } = renderHook(() => useAccountProfile());

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.profile).toBeNull();
    expect(result.current.error).toContain("no network");
  });
});

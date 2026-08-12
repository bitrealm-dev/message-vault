/** @vitest-environment jsdom */
import { describe, it, expect } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useResource } from "./useResource";

describe("useResource", () => {
  it("loads data for a key", async () => {
    const { result } = renderHook(() =>
      useResource("k1", async () => "hello"),
    );

    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.data).toBe("hello");
    expect(result.current.error).toBe("");
  });

  it("surfaces fetch errors", async () => {
    const { result } = renderHook(() =>
      useResource("k1", async () => {
        throw new Error("nope");
      }),
    );

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.data).toBeNull();
    expect(result.current.error).toContain("nope");
  });

  it("clears state when key becomes null", async () => {
    const { result, rerender } = renderHook(
      ({ key }: { key: string | null }) =>
        useResource(key, async () => "hello"),
      { initialProps: { key: "k1" as string | null } },
    );

    await waitFor(() => expect(result.current.data).toBe("hello"));

    act(() => {
      rerender({ key: null });
    });

    expect(result.current.data).toBeNull();
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBe("");
  });
});

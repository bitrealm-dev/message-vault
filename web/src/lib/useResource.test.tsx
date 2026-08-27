/** @vitest-environment jsdom */

import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useResource } from "./useResource";

describe("useResource", () => {
  it("loads data for a key", async () => {
    const { result } = renderHook(() => useResource("k1", async () => "hello"));

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
      ({ key }: { key: string | null }) => useResource(key, async () => "hello"),
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

  it("refetches when reload is called", async () => {
    let n = 0;
    const { result } = renderHook(() =>
      useResource("k1", async () => {
        n += 1;
        return `v${n}`;
      }),
    );

    await waitFor(() => expect(result.current.data).toBe("v1"));

    act(() => {
      result.current.reload();
    });

    await waitFor(() => expect(result.current.data).toBe("v2"));
  });
});

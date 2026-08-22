/** @vitest-environment jsdom */

import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { usePagedList } from "./usePagedList";

describe("usePagedList", () => {
  it("reloads the first page when queryKey changes", async () => {
    const fetchPage = vi.fn(async ({ offset }: { offset: number }) => {
      const q = fetchPage.mock.calls.length;
      return {
        items: [{ id: `q${q}-o${offset}` }],
        total: 1,
      };
    });

    const { result, rerender } = renderHook(
      ({ queryKey }: { queryKey: string }) => usePagedList(queryKey, fetchPage),
      { initialProps: { queryKey: "alpha" } },
    );

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.items[0]?.id).toMatch(/^q1-/);
    const callsAfterFirst = fetchPage.mock.calls.length;

    act(() => {
      rerender({ queryKey: "beta" });
    });

    await waitFor(() => expect(fetchPage.mock.calls.length).toBeGreaterThan(callsAfterFirst));
    await waitFor(() => expect(result.current.loading || result.current.refreshing).toBe(false));
    expect(result.current.items[0]?.id).not.toMatch(/^q1-/);
  });
});

/** @vitest-environment jsdom */
import { describe, it, expect } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useAsyncAction } from "./useAsyncAction";

describe("useAsyncAction", () => {
  it("sets busy while running and clears on success", async () => {
    const { result } = renderHook(() => useAsyncAction());
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });

    let runPromise!: Promise<void>;
    act(() => {
      runPromise = result.current.run(async () => {
        await gate;
      });
    });

    await waitFor(() => expect(result.current.busy).toBe(true));
    expect(result.current.error).toBe("");

    await act(async () => {
      release();
      await runPromise;
    });

    expect(result.current.busy).toBe(false);
    expect(result.current.error).toBe("");
  });

  it("captures errors and clearError resets them", async () => {
    const { result } = renderHook(() => useAsyncAction());

    await act(async () => {
      await result.current.run(async () => {
        throw new Error("boom");
      });
    });

    expect(result.current.busy).toBe(false);
    expect(result.current.error).toContain("boom");

    act(() => {
      result.current.clearError();
    });
    expect(result.current.error).toBe("");
  });
});

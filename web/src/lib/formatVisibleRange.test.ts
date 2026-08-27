import { describe, expect, it } from "vitest";
import { formatVisibleRange, listActivitySuffix } from "./usePagedList";

describe("formatVisibleRange", () => {
  it("handles empty totals", () => {
    expect(formatVisibleRange(0, 0, 0, 0)).toBe("0 of 0");
    expect(formatVisibleRange(0, 0, 10, 0)).toBe("0 of 10");
  });

  it("shows ellipsis before the viewport is measured", () => {
    expect(formatVisibleRange(0, 0, 100, 40)).toBe("… of 100");
    expect(formatVisibleRange(-1, 5, 100, 40)).toBe("… of 100");
  });

  it("formats a clamped inclusive window", () => {
    expect(formatVisibleRange(1, 20, 100, 40)).toBe("1–20 of 100");
    expect(formatVisibleRange(1, 100, 100, 40)).toBe("1–40 of 100");
  });
});

describe("listActivitySuffix", () => {
  it("prefers updating over loading more", () => {
    expect(listActivitySuffix(true, true)).toBe(" · updating…");
    expect(listActivitySuffix(true, false)).toBe(" · updating…");
    expect(listActivitySuffix(false, true)).toBe(" · loading more…");
    expect(listActivitySuffix(false, false)).toBe("");
  });
});

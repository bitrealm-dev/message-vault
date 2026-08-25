/** @vitest-environment jsdom */

import { describe, expect, it } from "vitest";
import { headerLabelMinWidth } from "./headerLabelMinWidth";

describe("headerLabelMinWidth", () => {
  it("returns a finite positive width", () => {
    const w = headerLabelMinWidth("Threads");
    expect(Number.isFinite(w)).toBe(true);
    expect(w).toBeGreaterThan(0);
  });

  it("gives a shorter label a narrower min than a longer label", () => {
    expect(headerLabelMinWidth("Threads")).toBeLessThan(headerLabelMinWidth("Direct Messages"));
  });
});

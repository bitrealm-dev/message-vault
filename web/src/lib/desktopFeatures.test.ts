import { describe, it, expect } from "vitest";
import { canUseImportExport } from "./desktopFeatures.ts";

describe("canUseImportExport", () => {
  it("is true only in the desktop app for a non-guest account", () => {
    expect(canUseImportExport(true, false)).toBe(true);
    expect(canUseImportExport(true, true)).toBe(false);
    expect(canUseImportExport(false, false)).toBe(false);
  });
});

import { describe, expect, it } from "vitest";
import { canUseImportExport, canUseImportExportWithProfile } from "./desktopFeatures.ts";

describe("canUseImportExport", () => {
  it("is true only in the desktop app", () => {
    expect(canUseImportExport(true)).toBe(true);
    expect(canUseImportExport(false)).toBe(false);
  });
});

describe("canUseImportExportWithProfile", () => {
  it("is false when the profile is missing even in the desktop app", () => {
    expect(canUseImportExportWithProfile(true, null)).toBe(false);
    expect(canUseImportExportWithProfile(true, undefined)).toBe(false);
  });

  it("is true only when a loaded profile is in the desktop app", () => {
    expect(canUseImportExportWithProfile(true, {})).toBe(true);
    expect(canUseImportExportWithProfile(false, {})).toBe(false);
  });
});

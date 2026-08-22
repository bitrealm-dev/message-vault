import { describe, expect, it } from "vitest";
import { canUseImportExport, canUseImportExportWithProfile } from "./desktopFeatures.ts";

describe("canUseImportExport", () => {
  it("is true only in the desktop app for a non-guest account", () => {
    expect(canUseImportExport(true, false)).toBe(true);
    expect(canUseImportExport(true, true)).toBe(false);
    expect(canUseImportExport(false, false)).toBe(false);
  });
});

describe("canUseImportExportWithProfile", () => {
  it("is false when the profile is missing even in the desktop app", () => {
    expect(canUseImportExportWithProfile(true, null)).toBe(false);
    expect(canUseImportExportWithProfile(true, undefined)).toBe(false);
  });

  it("is true only when a loaded non-guest profile is in the desktop app", () => {
    expect(canUseImportExportWithProfile(true, { is_guest: false })).toBe(true);
    expect(canUseImportExportWithProfile(true, {})).toBe(true);
    expect(canUseImportExportWithProfile(true, { is_guest: true })).toBe(false);
    expect(canUseImportExportWithProfile(false, { is_guest: false })).toBe(false);
  });
});

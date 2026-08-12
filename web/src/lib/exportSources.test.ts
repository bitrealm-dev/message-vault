import { describe, it, expect } from "vitest";
import { EXPORT_SOURCES } from "./exportSources";

describe("EXPORT_SOURCES", () => {
  it("includes primary extract/import sources with unique ids", () => {
    const ids = EXPORT_SOURCES.map((s) => s.id);
    expect(ids).toContain("imessage-ios");
    expect(ids).toContain("whatsapp-android");
    expect(ids).toContain("sms-backup-restore");
    expect(new Set(ids).size).toBe(ids.length);
    for (const s of EXPORT_SOURCES) {
      expect(s.label.length).toBeGreaterThan(0);
    }
  });
});

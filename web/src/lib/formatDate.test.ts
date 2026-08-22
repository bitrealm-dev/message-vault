import { describe, expect, it } from "vitest";
import {
  formatDateSpan,
  formatDay,
  formatIsoDateOnly,
  formatMonthYear,
  formatUnixDate,
} from "./formatDate.ts";

describe("formatDay", () => {
  it("includes year, month, and day", () => {
    const out = formatDay("2024-09-09T12:00:00.000Z");
    expect(out).toMatch(/2024/);
    expect(out).toMatch(/9/);
  });
});

describe("formatMonthYear", () => {
  it("omits the day", () => {
    const out = formatMonthYear("2024-09-09T12:00:00.000Z");
    expect(out).toMatch(/2024/);
    expect(out).not.toMatch(/^\d/);
  });
});

describe("formatDateSpan", () => {
  it("returns null when both ends are missing", () => {
    expect(formatDateSpan(null, null)).toBeNull();
  });

  it("returns a single day when only one end is set", () => {
    expect(formatDateSpan("2024-09-09T12:00:00.000Z", null)).toBe(
      formatDay("2024-09-09T12:00:00.000Z"),
    );
    expect(formatDateSpan(null, "2024-09-10T12:00:00.000Z")).toBe(
      formatDay("2024-09-10T12:00:00.000Z"),
    );
  });

  it("collapses identical formatted ends", () => {
    const day = "2024-06-15T12:00:00.000Z";
    expect(formatDateSpan(day, day)).toBe(formatDay(day));
  });

  it("joins distinct formatted ends with an en dash", () => {
    const start = "2024-01-01T12:00:00.000Z";
    const end = "2024-12-31T12:00:00.000Z";
    expect(formatDateSpan(start, end)).toBe(`${formatDay(start)} – ${formatDay(end)}`);
  });
});

describe("formatUnixDate", () => {
  it("returns Never for missing or invalid values", () => {
    expect(formatUnixDate(null)).toBe("Never");
    expect(formatUnixDate("")).toBe("Never");
    expect(formatUnixDate("0")).toBe("Never");
    expect(formatUnixDate("abc")).toBe("Never");
  });

  it("formats positive unix seconds", () => {
    expect(formatUnixDate("1700000000")).toBe(new Date(1700000000 * 1000).toLocaleDateString());
  });
});

describe("formatIsoDateOnly", () => {
  it("returns null for empty input", () => {
    expect(formatIsoDateOnly(null)).toBeNull();
    expect(formatIsoDateOnly(undefined)).toBeNull();
  });

  it("formats parseable ISO as UTC YYYY-MM-DD", () => {
    expect(formatIsoDateOnly("2024-09-09T15:30:00.000Z")).toBe("2024-09-09");
  });

  it("extracts leading date from unparseable strings", () => {
    expect(formatIsoDateOnly("2024-09-09")).toBe("2024-09-09");
  });
});

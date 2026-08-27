import { describe, expect, it } from "vitest";
import {
  formatThemeShare,
  normalizeHex,
  parseThemeShare,
  resolveMode,
  type ThemeSeeds,
} from "./theme";

const seeds: ThemeSeeds = {
  lightHeader: "#112233",
  lightAccent: "#445566",
  darkHeader: "#778899",
  darkAccent: "#aabbcc",
};

describe("normalizeHex", () => {
  it("expands 3-digit hex and lowercases", () => {
    expect(normalizeHex("#AbC")).toBe("#aabbcc");
  });

  it("accepts 6-digit hex", () => {
    expect(normalizeHex(" #AABBCC ")).toBe("#aabbcc");
  });

  it("rejects invalid input", () => {
    expect(normalizeHex("red")).toBeNull();
    expect(normalizeHex("#gg0000")).toBeNull();
    expect(normalizeHex("")).toBeNull();
  });
});

describe("formatThemeShare / parseThemeShare", () => {
  it("round-trips four hex colors", () => {
    const share = formatThemeShare(seeds);
    expect(share).toBe("#112233,#445566,#778899,#aabbcc");
    expect(parseThemeShare(share)).toEqual(seeds);
  });

  it("parses whitespace-separated shares", () => {
    expect(parseThemeShare("#112233 #445566 #778899 #aabbcc")).toEqual(seeds);
  });

  it("returns null for wrong arity or bad hex", () => {
    expect(parseThemeShare("#112233,#445566")).toBeNull();
    expect(parseThemeShare("#112233,#445566,#778899,nope")).toBeNull();
  });
});

describe("resolveMode", () => {
  it("passes through light and dark", () => {
    expect(resolveMode("light", true)).toBe("light");
    expect(resolveMode("dark", false)).toBe("dark");
  });

  it("resolves system from prefersDark", () => {
    expect(resolveMode("system", true)).toBe("dark");
    expect(resolveMode("system", false)).toBe("light");
  });
});

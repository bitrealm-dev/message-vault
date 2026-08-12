import { describe, it, expect } from "vitest";
import { parseSelectKey } from "./selectKey.ts";

const MODES = ["copy", "convert", "skip"] as const;

describe("parseSelectKey", () => {
  it("returns the key when it is allowed", () => {
    expect(parseSelectKey("copy", MODES)).toBe("copy");
    expect(parseSelectKey(1, ["1", "2"] as const)).toBe("1");
  });

  it("returns null for unknown keys", () => {
    expect(parseSelectKey("invalid", MODES)).toBeNull();
    expect(parseSelectKey("", MODES)).toBeNull();
  });

  it("returns null for null input", () => {
    expect(parseSelectKey(null, MODES)).toBeNull();
  });
});

import { describe, expect, it } from "vitest";
import { commitPhoneTokens, removePhoneToken, splitPhoneTokenInput } from "./phoneTokens";

describe("splitPhoneTokenInput", () => {
  it("splits on commas and keeps spaces inside a number", () => {
    expect(splitPhoneTokenInput(" +1 555-1111, +15552222  ")).toEqual(["+1 555-1111", "+15552222"]);
  });

  it("returns empty for blank input", () => {
    expect(splitPhoneTokenInput("  ,  ")).toEqual([]);
  });
});

describe("commitPhoneTokens", () => {
  it("appends new phones and skips duplicates", () => {
    expect(commitPhoneTokens(["+15551111"], "+15551111, +15552222")).toEqual([
      "+15551111",
      "+15552222",
    ]);
  });
});

describe("removePhoneToken", () => {
  it("removes the first matching value", () => {
    expect(removePhoneToken(["a", "b", "a"], "a")).toEqual(["b", "a"]);
  });
});

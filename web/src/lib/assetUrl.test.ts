import { describe, expect, it } from "vitest";
import { buildAssetPath } from "./assetUrl.ts";

describe("buildAssetPath", () => {
  it("includes sha and required source query", () => {
    expect(buildAssetPath("abc123", "imessage")).toBe("/v1/assets/abc123?source=imessage");
  });

  it("encodes special characters", () => {
    expect(buildAssetPath("deadbeef", "sms backup")).toBe(
      "/v1/assets/deadbeef?source=sms%20backup",
    );
  });

  it("rejects empty sha or source", () => {
    expect(() => buildAssetPath("", "imessage")).toThrow();
    expect(() => buildAssetPath("abc", "")).toThrow();
  });
});

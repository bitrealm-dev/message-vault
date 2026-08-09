import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { buildAssetPath } from "./assetUrl.ts";

describe("buildAssetPath", () => {
  it("includes sha and required source query", () => {
    assert.equal(
      buildAssetPath("abc123", "imessage"),
      "/v1/assets/abc123?source=imessage",
    );
  });

  it("encodes special characters", () => {
    assert.equal(
      buildAssetPath("deadbeef", "sms backup"),
      "/v1/assets/deadbeef?source=sms%20backup",
    );
  });

  it("rejects empty sha or source", () => {
    assert.throws(() => buildAssetPath("", "imessage"));
    assert.throws(() => buildAssetPath("abc", ""));
  });
});

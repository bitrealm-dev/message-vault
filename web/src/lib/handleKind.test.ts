import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { inferHandleType, normalizeHandle } from "./handleKind";

describe("normalizeHandle guarded phone policy", () => {
  it("normalizes unambiguous US and +-prefixed values to E.164", () => {
    assert.equal(normalizeHandle("5555550100", "phone"), "+15555550100");
    assert.equal(normalizeHandle("15555550100", "phone"), "+15555550100");
    assert.equal(normalizeHandle("+15555550100", "phone"), "+15555550100");
    assert.equal(normalizeHandle("+44 20 7183 8750", "phone"), "+442071838750");
  });

  it("keeps ambiguous national numbers as digits (never +0…)", () => {
    // Trunk-zero national number: matches the import-time normalized form so
    // the review-flagged row is found, instead of fabricating +02079460000.
    assert.equal(normalizeHandle("020 7946 0000", "phone"), "02079460000");
    assert.equal(normalizeHandle("442079460000", "phone"), "442079460000");
    // Short codes stay as-is.
    assert.equal(normalizeHandle("7535", "phone"), "7535");
  });

  it("infers handle types from shape", () => {
    assert.equal(inferHandleType("020 7946 0000"), "phone");
    assert.equal(inferHandleType("+442071838750"), "phone");
    assert.equal(inferHandleType("person@example.com"), "email");
    assert.equal(inferHandleType("discord#1234"), "other");
  });
});

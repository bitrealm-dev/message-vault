import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { formatPhoneDisplay } from "./phoneE164";

describe("formatPhoneDisplay", () => {
  it("formats US E.164 with international spacing", () => {
    assert.equal(formatPhoneDisplay("+19412660605"), "+1 941 266 0605");
  });

  it("formats UK E.164 with international spacing", () => {
    assert.equal(formatPhoneDisplay("+447911123456"), "+44 7911 123456");
  });

  it("leaves emails unchanged", () => {
    assert.equal(
      formatPhoneDisplay("annette@example.com"),
      "annette@example.com",
    );
  });

  it("returns the original string when unparseable", () => {
    assert.equal(formatPhoneDisplay("not-a-phone"), "not-a-phone");
    assert.equal(formatPhoneDisplay(""), "");
    assert.equal(formatPhoneDisplay(null), "");
  });
});

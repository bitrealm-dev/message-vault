import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  hashPassword,
  passwordsMatch,
  verifyPassword,
} from "./password";
import { validatePasswordPlaintext } from "./passwordPolicy";

describe("password helpers", () => {
  it("rejects blank and overly long plaintext", () => {
    assert.equal(validatePasswordPlaintext(""), "password is required");
    assert.equal(
      validatePasswordPlaintext("x".repeat(100)),
      "password must be less than 100 characters",
    );
    assert.equal(validatePasswordPlaintext("ok"), null);
    assert.equal(validatePasswordPlaintext("x".repeat(99)), null);
  });

  it("hashes and verifies passwords", async () => {
    const hash = await hashPassword("secret-pass");
    assert.notEqual(hash, "secret-pass");
    assert.equal(await verifyPassword(hash, "secret-pass"), true);
    assert.equal(await verifyPassword(hash, "wrong"), false);
  });

  it("treats null hash as no-password (empty only)", async () => {
    assert.equal(await passwordsMatch(null, ""), true);
    assert.equal(await passwordsMatch(undefined, ""), true);
    assert.equal(await passwordsMatch(null, "x"), false);
  });

  it("requires matching password when hash is set", async () => {
    const hash = await hashPassword("hunter2");
    assert.equal(await passwordsMatch(hash, "hunter2"), true);
    assert.equal(await passwordsMatch(hash, ""), false);
    assert.equal(await passwordsMatch(hash, "nope"), false);
  });
});

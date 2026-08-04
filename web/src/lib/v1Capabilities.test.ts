import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  DELETION_UI_ENABLED,
  isDeletionUiBlocked,
  isDeletionUiEnabled,
} from "./v1Capabilities";

describe("v1Capabilities", () => {
  it("keeps deletion UI disabled for V1", () => {
    assert.equal(DELETION_UI_ENABLED, false);
    assert.equal(isDeletionUiEnabled(), false);
    assert.equal(isDeletionUiBlocked(), true);
  });
});

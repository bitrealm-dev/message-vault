import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  isSettingsPath,
  SETTINGS_TABS,
  settingsTabActive,
} from "./settingsNav";

describe("settingsNav", () => {
  it("recognizes settings routes", () => {
    assert.equal(isSettingsPath("/settings"), true);
    assert.equal(isSettingsPath("/settings/account"), true);
    assert.equal(isSettingsPath("/settings/display"), true);
    assert.equal(isSettingsPath("/all"), false);
    assert.equal(isSettingsPath("/"), false);
  });

  it("selects Account for both account and redirect paths", () => {
    assert.equal(settingsTabActive("/settings/account", "/settings/account"), true);
    assert.equal(settingsTabActive("/settings", "/settings/account"), true);
    assert.equal(settingsTabActive("/settings/display", "/settings/account"), false);
  });

  it("selects Appearance only on the display route", () => {
    assert.equal(settingsTabActive("/settings/display", "/settings/display"), true);
    assert.equal(settingsTabActive("/settings/account", "/settings/display"), false);
  });

  it("exposes Account and Appearance tabs", () => {
    assert.deepEqual(
      SETTINGS_TABS.map((t) => t.label),
      ["Account", "Appearance"],
    );
  });
});

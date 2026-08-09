import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  currentAppUrl,
  isSettingsPath,
  isValidSettingsReturnTo,
  resolveSettingsReturnTo,
  settingsAccountHref,
  settingsLinkFromLocation,
  SETTINGS_TABS,
  settingsTabActive,
  settingsTabHref,
} from "./settingsNav";

describe("settingsNav", () => {
  it("recognizes settings routes", () => {
    assert.equal(isSettingsPath("/settings"), true);
    assert.equal(isSettingsPath("/settings/account"), true);
    assert.equal(isSettingsPath("/settings/access"), true);
    assert.equal(isSettingsPath("/settings/storage"), true);
    assert.equal(isSettingsPath("/settings/display"), true);
    assert.equal(isSettingsPath("/all"), false);
    assert.equal(isSettingsPath("/"), false);
  });

  it("selects Account for both account and redirect paths", () => {
    assert.equal(settingsTabActive("/settings/account", "/settings/account"), true);
    assert.equal(settingsTabActive("/settings", "/settings/account"), true);
    assert.equal(settingsTabActive("/settings/display", "/settings/account"), false);
    assert.equal(settingsTabActive("/settings/access", "/settings/account"), false);
  });

  it("selects Access only on the access route", () => {
    assert.equal(settingsTabActive("/settings/access", "/settings/access"), true);
    assert.equal(settingsTabActive("/settings/account", "/settings/access"), false);
  });

  it("selects Storage only on the storage route", () => {
    assert.equal(settingsTabActive("/settings/storage", "/settings/storage"), true);
    assert.equal(settingsTabActive("/settings/account", "/settings/storage"), false);
  });

  it("selects Appearance only on the display route", () => {
    assert.equal(settingsTabActive("/settings/display", "/settings/display"), true);
    assert.equal(settingsTabActive("/settings/account", "/settings/display"), false);
  });

  it("exposes Account, Access, Storage, and Appearance tabs", () => {
    assert.deepEqual(
      SETTINGS_TABS.map((t) => t.label),
      ["Account", "Access", "Storage", "Appearance"],
    );
  });

  it("builds settings links with encoded returnTo", () => {
    assert.equal(
      settingsAccountHref("/all?c=42&q=hello"),
      "/settings/account?returnTo=%2Fall%3Fc%3D42%26q%3Dhello",
    );
    assert.equal(settingsAccountHref("/settings/account"), "/settings/account");
    assert.equal(settingsAccountHref("https://evil.test"), "/settings/account");
  });

  it("validates internal non-settings return targets", () => {
    assert.equal(isValidSettingsReturnTo("/all"), true);
    assert.equal(isValidSettingsReturnTo("/all?c=1"), true);
    assert.equal(isValidSettingsReturnTo("/settings/account"), false);
    assert.equal(isValidSettingsReturnTo("//evil.test"), false);
    assert.equal(isValidSettingsReturnTo("https://evil.test"), false);
  });

  it("resolves returnTo with /all fallback", () => {
    assert.equal(resolveSettingsReturnTo(null), "/all");
    assert.equal(resolveSettingsReturnTo("%2Fall%3Fc%3D3"), "/all?c=3");
    assert.equal(resolveSettingsReturnTo("%2Fsettings%2Faccount"), "/all");
  });

  it("preserves returnTo across settings tabs", () => {
    const returnTo = "%2Fall%3Fc%3D3";
    assert.equal(
      settingsTabHref("/settings/display", returnTo),
      "/settings/display?returnTo=%2Fall%3Fc%3D3",
    );
    assert.equal(
      settingsTabHref("/settings/storage", returnTo),
      "/settings/storage?returnTo=%2Fall%3Fc%3D3",
    );
    assert.equal(settingsTabHref("/settings/account", null), "/settings/account");
  });

  it("captures browse location for settings sidebar link", () => {
    const params = new URLSearchParams("c=7&q=test");
    assert.equal(
      settingsLinkFromLocation("/all", params),
      "/settings/account?returnTo=%2Fall%3Fc%3D7%26q%3Dtest",
    );
    assert.equal(
      settingsLinkFromLocation("/settings/display", new URLSearchParams("returnTo=%2Ftrash")),
      "/settings/account?returnTo=%2Ftrash",
    );
  });

  it("formats current app URLs", () => {
    assert.equal(currentAppUrl("/all", "c=1"), "/all?c=1");
    assert.equal(currentAppUrl("/trash", ""), "/trash");
  });
});

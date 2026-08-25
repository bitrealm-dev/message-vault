/** @vitest-environment jsdom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clampWidth, loadWidth, saveWidth } from "./columnResize";
import {
  LEFT_PANEL_DEFAULT_WIDTH,
  LEFT_PANEL_MAX_WIDTH,
  LEFT_PANEL_MIN_WIDTH,
  LEFT_PANEL_STORAGE_KEY,
} from "./leftPanelWidth";

describe("clampWidth", () => {
  it("rounds and clamps to the inclusive range", () => {
    expect(clampWidth(100, LEFT_PANEL_MIN_WIDTH, LEFT_PANEL_MAX_WIDTH)).toBe(LEFT_PANEL_MIN_WIDTH);
    expect(clampWidth(600, LEFT_PANEL_MIN_WIDTH, LEFT_PANEL_MAX_WIDTH)).toBe(LEFT_PANEL_MAX_WIDTH);
    expect(clampWidth(220.6, LEFT_PANEL_MIN_WIDTH, LEFT_PANEL_MAX_WIDTH)).toBe(221);
    expect(clampWidth(220.4, LEFT_PANEL_MIN_WIDTH, LEFT_PANEL_MAX_WIDTH)).toBe(220);
  });
});

describe("loadWidth", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
  });

  it("returns defaultWidth when nothing is stored", () => {
    expect(loadWidth("testWidth:v1", 300, 220, 560)).toBe(300);
  });

  it("returns the clamped stored value", () => {
    localStorage.setItem("testWidth:v1", "350");
    expect(loadWidth("testWidth:v1", 300, 220, 560)).toBe(350);
  });

  it("clamps values outside the allowed range", () => {
    localStorage.setItem("testWidth:v1", "100");
    expect(loadWidth("testWidth:v1", 300, 220, 560)).toBe(220);
    localStorage.setItem("testWidth:v1", "999");
    expect(loadWidth("testWidth:v1", 300, 220, 560)).toBe(560);
  });

  it("clamps nav panel widths to the shared left-panel bounds", () => {
    localStorage.setItem(LEFT_PANEL_STORAGE_KEY, "100");
    expect(
      loadWidth(
        LEFT_PANEL_STORAGE_KEY,
        LEFT_PANEL_DEFAULT_WIDTH,
        LEFT_PANEL_MIN_WIDTH,
        LEFT_PANEL_MAX_WIDTH,
      ),
    ).toBe(LEFT_PANEL_MIN_WIDTH);
    localStorage.setItem(LEFT_PANEL_STORAGE_KEY, "999");
    expect(
      loadWidth(
        LEFT_PANEL_STORAGE_KEY,
        LEFT_PANEL_DEFAULT_WIDTH,
        LEFT_PANEL_MIN_WIDTH,
        LEFT_PANEL_MAX_WIDTH,
      ),
    ).toBe(LEFT_PANEL_MAX_WIDTH);
  });

  it("returns defaultWidth for non-numeric storage", () => {
    localStorage.setItem("testWidth:v1", "nope");
    expect(loadWidth("testWidth:v1", 300, 220, 560)).toBe(300);
  });

  it("returns defaultWidth when localStorage throws", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("blocked");
    });
    expect(loadWidth("testWidth:v1", 300, 220, 560)).toBe(300);
    vi.restoreAllMocks();
  });
});

describe("saveWidth", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    localStorage.clear();
    vi.restoreAllMocks();
  });

  it("writes the width as a string", () => {
    saveWidth("testWidth:v1", 280);
    expect(localStorage.getItem("testWidth:v1")).toBe("280");
  });

  it("swallows quota / private-browsing errors", () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("quota");
    });
    expect(() => saveWidth("testWidth:v1", 280)).not.toThrow();
  });
});

/** @vitest-environment jsdom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clampWidth, loadWidth, saveWidth } from "./columnResize";

describe("clampWidth", () => {
  it("rounds and clamps to the inclusive range", () => {
    expect(clampWidth(100, 160, 360)).toBe(160);
    expect(clampWidth(400, 160, 360)).toBe(360);
    expect(clampWidth(220.6, 160, 360)).toBe(221);
    expect(clampWidth(220.4, 160, 360)).toBe(220);
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

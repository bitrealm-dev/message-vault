import { describe, expect, it } from "vitest";
import { sameFolder } from "./convertUtils";

describe("sameFolder", () => {
  it("treats identical paths as the same folder", () => {
    expect(sameFolder("/home/demo/out", "/home/demo/out")).toBe(true);
  });

  it("ignores surrounding whitespace and trailing slashes", () => {
    expect(sameFolder(" /home/demo/out/ ", "/home/demo/out")).toBe(true);
    expect(sameFolder("C:\\exports\\", "C:\\exports")).toBe(true);
  });

  it("does not call two empty fields the same folder", () => {
    // Both fields start empty. The button is disabled for emptiness, not for
    // a folder clash, so no clash message should show yet.
    expect(sameFolder("", "")).toBe(false);
    expect(sameFolder("  ", "")).toBe(false);
  });

  it("keeps a bare root distinct from an empty field", () => {
    expect(sameFolder("/", "")).toBe(false);
    expect(sameFolder("/", "/")).toBe(true);
  });

  it("treats different paths as different folders", () => {
    expect(sameFolder("/home/demo/out", "/home/demo/out2")).toBe(false);
    expect(sameFolder("/home/demo/in", "/home/demo/in/sub")).toBe(false);
  });
});

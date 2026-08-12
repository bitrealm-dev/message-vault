/** @vitest-environment jsdom */
import { describe, it, expect, beforeEach } from "vitest";
import {
  personDisplayLabel,
  readUseNameAliases,
  writeUseNameAliases,
  USE_NAME_ALIASES_KEY,
} from "./nameAliases";

describe("personDisplayLabel", () => {
  it("prefers preferred name when aliases are off", () => {
    expect(
      personDisplayLabel(
        { preferredName: "Ada", nameAlias: "A.L.", handle: "+1" },
        false,
      ),
    ).toBe("Ada");
  });

  it("prefers alias when aliases are on", () => {
    expect(
      personDisplayLabel(
        { preferredName: "Ada", nameAlias: "A.L.", handle: "+1" },
        true,
      ),
    ).toBe("A.L.");
  });

  it("falls back to handle", () => {
    expect(
      personDisplayLabel({ preferredName: null, nameAlias: null, handle: "+1" }, true),
    ).toBe("+1");
  });
});

describe("name alias preference storage", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("reads and writes the preference flag", () => {
    expect(readUseNameAliases()).toBe(false);
    writeUseNameAliases(true);
    expect(window.localStorage.getItem(USE_NAME_ALIASES_KEY)).toBe("1");
    expect(readUseNameAliases()).toBe(true);
    writeUseNameAliases(false);
    expect(window.localStorage.getItem(USE_NAME_ALIASES_KEY)).toBeNull();
  });
});

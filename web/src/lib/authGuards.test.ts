import { describe, it, expect } from "vitest";
import { isAuthMode, parsePersistedAuth } from "./authGuards.ts";

describe("isAuthMode", () => {
  it("accepts hanko and local", () => {
    expect(isAuthMode("hanko")).toBe(true);
    expect(isAuthMode("local")).toBe(true);
  });

  it("rejects other values", () => {
    expect(isAuthMode(null)).toBe(false);
    expect(isAuthMode("oauth")).toBe(false);
    expect(isAuthMode(1)).toBe(false);
  });
});

describe("parsePersistedAuth", () => {
  it("parses valid persisted auth", () => {
    expect(
      parsePersistedAuth(
        JSON.stringify({
          serverUrl: "http://localhost:8080",
          token: "tok",
          accountId: "acc1",
          needsOnboarding: true,
        }),
      ),
    ).toEqual({
      serverUrl: "http://localhost:8080",
      token: "tok",
      accountId: "acc1",
      needsOnboarding: true,
    });
  });

  it("returns null for corrupt or incomplete JSON", () => {
    expect(parsePersistedAuth("{")).toBeNull();
    expect(parsePersistedAuth("not json")).toBeNull();
    expect(parsePersistedAuth(JSON.stringify({ token: "t" }))).toBeNull();
    expect(parsePersistedAuth(JSON.stringify({ serverUrl: "", token: "", accountId: "" }))).toBeNull();
  });
});

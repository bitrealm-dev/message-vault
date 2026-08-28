import { describe, expect, it } from "vitest";
import {
  DEFAULT_TAURI_VAULT_URL,
  initialLoginServerUrl,
  isAuthMode,
  isTryDemoEnabled,
  parsePersistedAuth,
  vaultDisplayHost,
} from "./authGuards.ts";

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

describe("initialLoginServerUrl", () => {
  it("defaults the desktop app to IPv4 loopback", () => {
    expect(initialLoginServerUrl(undefined, true)).toBe(DEFAULT_TAURI_VAULT_URL);
    expect(initialLoginServerUrl("", true)).toBe(DEFAULT_TAURI_VAULT_URL);
  });

  it("leaves the browser field blank so the page origin is used", () => {
    expect(initialLoginServerUrl(undefined, false)).toBe("");
    expect(initialLoginServerUrl("", false)).toBe("");
  });

  it("rewrites the old localhost default and keeps any other saved URL", () => {
    expect(initialLoginServerUrl("http://localhost:8080", true)).toBe(DEFAULT_TAURI_VAULT_URL);
    expect(initialLoginServerUrl("http://localhost:8080/", false)).toBe(DEFAULT_TAURI_VAULT_URL);
    expect(initialLoginServerUrl("https://vault.example.com", true)).toBe(
      "https://vault.example.com",
    );
  });
});

describe("isTryDemoEnabled", () => {
  it("reads try_demo only when true", () => {
    expect(isTryDemoEnabled(true)).toBe(true);
    expect(isTryDemoEnabled(false)).toBe(false);
    expect(isTryDemoEnabled(undefined)).toBe(false);
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
    expect(
      parsePersistedAuth(JSON.stringify({ serverUrl: "", token: "", accountId: "" })),
    ).toBeNull();
  });
});

describe("vaultDisplayHost", () => {
  it("shows this page's host for a blank address", () => {
    expect(vaultDisplayHost("", "vault.bitrealm.io")).toBe("vault.bitrealm.io");
  });

  it("shows host and port for an absolute address", () => {
    expect(vaultDisplayHost("http://127.0.0.1:8080", "example.test")).toBe("127.0.0.1:8080");
  });

  it("drops a trailing path and slash", () => {
    expect(vaultDisplayHost("https://vault.example.com/", "example.test")).toBe(
      "vault.example.com",
    );
  });

  it("shows an unparseable address as typed, rather than nothing", () => {
    expect(vaultDisplayHost("not a url", "example.test")).toBe("not a url");
  });
});

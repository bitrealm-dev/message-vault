import { describe, it, expect, beforeEach } from "vitest";
import {
  CONTACT_RECENT_SEARCHES_KEY,
  clearContactRecentSearches,
  loadContactRecentSearches,
  pushContactRecentSearch,
} from "./contactRecentSearches.ts";

const mem = new Map<string, string>();

beforeEach(() => {
  mem.clear();
  (globalThis as { localStorage?: Storage }).localStorage = {
    getItem: (k) => mem.get(k) ?? null,
    setItem: (k, v) => {
      mem.set(k, String(v));
    },
    removeItem: (k) => {
      mem.delete(k);
    },
    clear: () => mem.clear(),
    key: () => null,
    length: 0,
  };
});

describe("contactRecentSearches", () => {
  it("returns empty for missing or corrupt JSON", () => {
    expect(loadContactRecentSearches()).toEqual([]);
    mem.set(CONTACT_RECENT_SEARCHES_KEY, "{not-json");
    expect(loadContactRecentSearches()).toEqual([]);
  });

  it("pushes newest first, dedupes, and caps at 10", () => {
    for (let i = 0; i < 12; i++) pushContactRecentSearch(`q${i}`);
    const list = loadContactRecentSearches();
    expect(list).toHaveLength(10);
    expect(list[0]).toBe("q11");
    expect(list[9]).toBe("q2");
    pushContactRecentSearch("q5");
    expect(loadContactRecentSearches()[0]).toBe("q5");
    expect(loadContactRecentSearches().filter((q) => q === "q5")).toHaveLength(
      1,
    );
  });

  it("ignores empty and whitespace-only pushes", () => {
    pushContactRecentSearch("  ");
    expect(loadContactRecentSearches()).toEqual([]);
  });

  it("clear removes the key", () => {
    pushContactRecentSearch("alice");
    clearContactRecentSearches();
    expect(loadContactRecentSearches()).toEqual([]);
  });
});

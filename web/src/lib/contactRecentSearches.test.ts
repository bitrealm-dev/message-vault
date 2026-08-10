import assert from "node:assert/strict";
import { describe, it, beforeEach } from "node:test";
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
    assert.deepEqual(loadContactRecentSearches(), []);
    mem.set(CONTACT_RECENT_SEARCHES_KEY, "{not-json");
    assert.deepEqual(loadContactRecentSearches(), []);
  });

  it("pushes newest first, dedupes, and caps at 10", () => {
    for (let i = 0; i < 12; i++) pushContactRecentSearch(`q${i}`);
    const list = loadContactRecentSearches();
    assert.equal(list.length, 10);
    assert.equal(list[0], "q11");
    assert.equal(list[9], "q2");
    pushContactRecentSearch("q5");
    assert.equal(loadContactRecentSearches()[0], "q5");
    assert.equal(loadContactRecentSearches().filter((q) => q === "q5").length, 1);
  });

  it("ignores empty and whitespace-only pushes", () => {
    pushContactRecentSearch("  ");
    assert.deepEqual(loadContactRecentSearches(), []);
  });

  it("clear removes the key", () => {
    pushContactRecentSearch("alice");
    clearContactRecentSearches();
    assert.deepEqual(loadContactRecentSearches(), []);
  });
});

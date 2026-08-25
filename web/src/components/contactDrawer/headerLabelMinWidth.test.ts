/** @vitest-environment jsdom */

import { describe, expect, it } from "vitest";
import { columnInitialWidth, headerLabelMinWidth } from "./headerLabelMinWidth";

describe("headerLabelMinWidth", () => {
  it("returns a finite positive width", () => {
    const w = headerLabelMinWidth("Threads");
    expect(Number.isFinite(w)).toBe(true);
    expect(w).toBeGreaterThan(0);
  });

  it("gives a shorter label a narrower min than a longer label", () => {
    expect(headerLabelMinWidth("Threads")).toBeLessThan(headerLabelMinWidth("Direct Messages"));
  });

  it("two-line Direct/Group min uses the longest line, not the joined phrase", () => {
    const twoLine = headerLabelMinWidth("Messages");
    const oneLine = headerLabelMinWidth("Direct Messages");
    expect(twoLine).toBeLessThan(oneLine);
  });

  it("compact count headers stay at header size; dates share a fixed width", () => {
    const threads = headerLabelMinWidth("Threads");
    const messages = headerLabelMinWidth("Messages");
    const firstSeen = headerLabelMinWidth("First Seen");
    const lastSeen = headerLabelMinWidth("Last Seen");
    expect(columnInitialWidth(threads, ["999", "1,234"])).toBe(threads);
    expect(columnInitialWidth(messages, ["39,098", "3,192"])).toBe(messages);
    const dateCol = columnInitialWidth(Math.max(firstSeen, lastSeen), ["2020-12-31"]);
    expect(dateCol).toBeGreaterThanOrEqual(firstSeen);
    expect(dateCol).toBeGreaterThanOrEqual(lastSeen);
  });
});

describe("columnInitialWidth", () => {
  it("is at least the header min when cells are empty or short", () => {
    const headerMin = headerLabelMinWidth("Threads");
    expect(columnInitialWidth(headerMin, ["1", "—"])).toBe(headerMin);
  });

  it("grows when a cell is wider than the header", () => {
    const headerMin = headerLabelMinWidth("Alias");
    const wide = columnInitialWidth(headerMin, ["Mary Elizabeth Katherine Thompson"]);
    expect(wide).toBeGreaterThan(headerMin);
  });
});

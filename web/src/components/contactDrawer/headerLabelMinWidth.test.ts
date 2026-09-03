/** @vitest-environment jsdom */

import { describe, expect, it } from "vitest";
import {
  columnInitialWidth,
  HANDLE_TABLE_COUNT_CELL_PADDING_PX,
  HANDLE_TABLE_DATE_SAMPLE,
  HANDLE_TABLE_GROUP_CELL_PADDING_PX,
  HANDLE_TABLE_MESSAGES_MAX,
  HANDLE_TABLE_THREADS_MAX,
  headerLabelMinWidth,
} from "./headerLabelMinWidth";

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

  it("sizes threads, messages, and dates from known maxima", () => {
    const threads = headerLabelMinWidth("Threads");
    const messages = headerLabelMinWidth("Messages");
    const firstSeen = headerLabelMinWidth("First Seen");
    const lastSeen = headerLabelMinWidth("Last Seen");
    expect(HANDLE_TABLE_THREADS_MAX).toBe("9,999");
    expect(HANDLE_TABLE_MESSAGES_MAX).toBe("999,999");
    expect(HANDLE_TABLE_DATE_SAMPLE).toBe("2020-12-31");
    expect(HANDLE_TABLE_COUNT_CELL_PADDING_PX).toBe(24);
    expect(HANDLE_TABLE_GROUP_CELL_PADDING_PX).toBe(48);
    expect(
      columnInitialWidth(threads, [HANDLE_TABLE_THREADS_MAX], HANDLE_TABLE_COUNT_CELL_PADDING_PX),
    ).toBeGreaterThanOrEqual(threads);
    expect(
      columnInitialWidth(messages, [HANDLE_TABLE_MESSAGES_MAX], HANDLE_TABLE_COUNT_CELL_PADDING_PX),
    ).toBeGreaterThanOrEqual(messages);
    const dateCol = columnInitialWidth(
      Math.max(firstSeen, lastSeen),
      [HANDLE_TABLE_DATE_SAMPLE],
      HANDLE_TABLE_COUNT_CELL_PADDING_PX,
    );
    expect(dateCol).toBeGreaterThanOrEqual(firstSeen);
    expect(dateCol).toBeGreaterThanOrEqual(lastSeen);
    const groupCol = columnInitialWidth(
      messages,
      [HANDLE_TABLE_MESSAGES_MAX],
      HANDLE_TABLE_GROUP_CELL_PADDING_PX,
    );
    expect(groupCol).toBeGreaterThanOrEqual(
      columnInitialWidth(messages, [HANDLE_TABLE_MESSAGES_MAX], HANDLE_TABLE_COUNT_CELL_PADDING_PX),
    );
  });
});

describe("columnInitialWidth", () => {
  it("is at least the header min when cells are empty or short", () => {
    const headerMin = headerLabelMinWidth("Threads");
    expect(columnInitialWidth(headerMin, ["1", "—"])).toBe(headerMin);
  });

  it("grows when a cell is wider than the header", () => {
    const headerMin = headerLabelMinWidth("Identity");
    const wide = columnInitialWidth(headerMin, ["Mary Elizabeth Katherine Thompson"]);
    expect(wide).toBeGreaterThan(headerMin);
  });
});

import { describe, expect, it } from "vitest";
import type { PushFinishedReport } from "../../lib/tauri";
import { importOutcome } from "./importOutcome";

function report(overrides: Partial<PushFinishedReport> = {}): PushFinishedReport {
  return {
    ok: true,
    messages: 100,
    messages_attempted: 100,
    messages_inserted: 100,
    messages_deduped: 0,
    messages_failed: 0,
    assets_uploaded: 5,
    assets_bytes: 1_000,
    conversations_ok: 10,
    conversations_total: 10,
    conversations_failed: 0,
    conversations_skipped: 0,
    results: [],
    ...overrides,
  };
}

describe("importOutcome", () => {
  it("is completed for a clean run", () => {
    expect(importOutcome({ report: report(), threw: false, issues: [] })).toBe("completed");
  });

  it("is failed when the job threw, whatever the report says", () => {
    expect(importOutcome({ report: report(), threw: true, issues: [] })).toBe("failed");
  });

  it("is failed when there is no report at all", () => {
    expect(importOutcome({ report: undefined, threw: false, issues: [] })).toBe("failed");
  });

  it("is failed when every conversation failed and nothing landed (2026-08-27 shape)", () => {
    const r = report({
      ok: false,
      conversations_total: 681,
      conversations_ok: 0,
      conversations_failed: 681,
      conversations_skipped: 0,
      messages_inserted: 0,
      messages_failed: 8_000,
    });
    expect(importOutcome({ report: r, threw: false, issues: [] })).toBe("failed");
  });

  it("is completed when a re-push dedupes everything to skips", () => {
    const r = report({
      conversations_total: 10,
      conversations_ok: 0,
      conversations_failed: 0,
      conversations_skipped: 10,
      messages_inserted: 0,
    });
    expect(importOutcome({ report: r, threw: false, issues: [] })).toBe("completed");
  });

  it("is completed_with_issues when some conversations failed but others landed", () => {
    const r = report({ ok: false, conversations_ok: 8, conversations_failed: 2 });
    expect(importOutcome({ report: r, threw: false, issues: [] })).toBe("completed_with_issues");
  });

  it("is completed_with_issues when messages failed inside ok conversations", () => {
    const r = report({ messages_failed: 3 });
    expect(importOutcome({ report: r, threw: false, issues: [] })).toBe("completed_with_issues");
  });

  it("is completed_with_issues when the run recorded an issue", () => {
    expect(importOutcome({ report: report(), threw: false, issues: [{ kind: "skip" }] })).toBe(
      "completed_with_issues",
    );
  });
});

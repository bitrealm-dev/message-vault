import { describe, expect, it } from "vitest";
import type { PushFinishedReport } from "./tauri";
import { parseTauriJobResult } from "./tauri";

function reportJson(overrides: Partial<PushFinishedReport> = {}): string {
  const report = {
    ok: true,
    messages: 10,
    messages_attempted: 10,
    messages_inserted: 10,
    messages_deduped: 0,
    messages_failed: 0,
    assets_uploaded: 1,
    assets_bytes: 100,
    conversations_ok: 5,
    conversations_total: 5,
    conversations_failed: 0,
    conversations_skipped: 0,
    results: [],
    ...overrides,
  };
  return JSON.stringify(report);
}

describe("parseTauriJobResult", () => {
  it("attaches a report that carries every field the verdict depends on", () => {
    const result = parseTauriJobResult(reportJson());
    expect(result.report).toBeDefined();
    expect(result.report?.conversations_failed).toBe(0);
    expect(result.report?.conversations_skipped).toBe(0);
  });

  // Regression for the last path back to the 2026-08-27 incident: a report
  // JSON blob missing conversations_failed/conversations_skipped must not
  // narrow to PushFinishedReport, or importOutcome computes
  // `undefined === 0` -> false and reports "completed" for a run where
  // nothing landed.
  it("does not attach a report missing conversations_failed", () => {
    const parsed: Record<string, unknown> = JSON.parse(reportJson());
    delete parsed.conversations_failed;
    const result = parseTauriJobResult(JSON.stringify(parsed));
    expect(result.report).toBeUndefined();
  });

  it("does not attach a report missing conversations_skipped", () => {
    const parsed: Record<string, unknown> = JSON.parse(reportJson());
    delete parsed.conversations_skipped;
    const result = parseTauriJobResult(JSON.stringify(parsed));
    expect(result.report).toBeUndefined();
  });

  it("still parses an extraction summary", () => {
    const result = parseTauriJobResult(
      JSON.stringify({ summary: "done", files_parsed: 3, messages_parsed: 20 }),
    );
    expect(result.extraction).toEqual({ files_parsed: 3, messages_parsed: 20 });
  });

  it("falls back to a plain summary for a non-JSON string", () => {
    const result = parseTauriJobResult("Extracted 10 messages.");
    expect(result).toEqual({ summary: "Extracted 10 messages." });
  });
});

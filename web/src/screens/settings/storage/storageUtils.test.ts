import { describe, it, expect } from "vitest";
import {
  formatBytes,
  formatImportDate,
  toImportSummaryView,
  type ImportDetailResponse,
} from "./storageUtils";

function detail(
  partial: Partial<ImportDetailResponse> = {},
): ImportDetailResponse {
  return {
    id: 1,
    source: "imessage-ios",
    tool: null,
    mode: "import",
    status: "completed",
    started_at: "2026-08-11T12:00:00Z",
    finished_at: "2026-08-11T12:01:00Z",
    message_count: 10,
    attachment_count: 0,
    bytes_uploaded: 0,
    duration_ms: 1000,
    parse_ms: null,
    convert_ms: null,
    upload_ms: null,
    summary: {},
    issues: [],
    ...partial,
  };
}

describe("formatBytes", () => {
  it("handles zero and non-finite", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-1)).toBe("0 B");
    expect(formatBytes(Number.NaN)).toBe("0 B");
  });

  it("scales units", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
  });
});

describe("formatImportDate", () => {
  it("returns em dash for empty", () => {
    expect(formatImportDate(null)).toBe("—");
    expect(formatImportDate(undefined)).toBe("—");
  });

  it("returns original string for invalid dates", () => {
    expect(formatImportDate("not-a-date")).toBe("not-a-date");
  });
});

describe("toImportSummaryView", () => {
  it("maps completed status and summary counts", () => {
    const view = toImportSummaryView(
      detail({
        summary: {
          files_total: 3,
          messages_inserted: 7,
          messages_deduped: 2,
        },
      }),
    );
    expect(view.status).toBe("completed");
    expect(view.filesTotal).toBe(3);
    expect(view.messagesInserted).toBe(7);
    expect(view.messagesDeduped).toBe(2);
  });

  it("treats unknown status as failed", () => {
    expect(toImportSummaryView(detail({ status: "exploded" })).status).toBe(
      "failed",
    );
  });

  it("falls back messagesInserted to message_count", () => {
    expect(toImportSummaryView(detail({ message_count: 42 })).messagesInserted).toBe(
      42,
    );
  });

  it("sums stage timings when duration_ms is missing", () => {
    const view = toImportSummaryView(
      detail({
        duration_ms: null,
        parse_ms: 10,
        convert_ms: 20,
        upload_ms: 30,
      }),
    );
    expect(view.durationMs).toBe(60);
  });
});

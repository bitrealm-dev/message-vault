/** @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import ImportSummaryPanel, { type ImportSummaryView } from "./ImportSummaryPanel";

const baseSummary: ImportSummaryView = {
  status: "completed",
  messagesParsed: 10,
  messagesAttempted: 10,
  messagesInserted: 9,
  messagesDeduped: 1,
  messagesFailed: 0,
  durationMs: 1000,
  issues: [],
};

describe("ImportSummaryPanel", () => {
  afterEach(() => {
    cleanup();
  });

  it("hides Import Errors when there are no issues", () => {
    render(<ImportSummaryPanel summary={baseSummary} embedStepTimings={false} />);
    expect(screen.queryByRole("heading", { name: "Import Errors" })).not.toBeInTheDocument();
    expect(screen.queryByText("Open import log")).not.toBeInTheDocument();
  });

  it("shows Import Errors heading and table when issues exist", () => {
    render(
      <ImportSummaryPanel
        summary={{
          ...baseSummary,
          issues: [
            {
              kind: "error",
              step: "upload",
              item: "thread.jsonl",
              reason: "HTTP 500 from vault",
            },
          ],
        }}
        embedStepTimings={false}
      />,
    );
    expect(screen.getByRole("heading", { name: "Import Errors" })).toBeInTheDocument();
    expect(screen.getByLabelText("Import errors")).toBeInTheDocument();
    expect(screen.queryByText("Open import log")).not.toBeInTheDocument();
  });
});

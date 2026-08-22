import { useVirtualizer } from "@tanstack/react-virtual";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import type { ImportIssue } from "./ImportSummaryPanel";

/** Collapsed row: file + step + two lines of error text. */
const COLLAPSED_ROW_HEIGHT = 56;
const MAX_VISIBLE_ROWS = 14;
const ISSUE_COLUMNS = "grid-cols-[minmax(0,1fr)_4.5rem_minmax(0,1.4fr)]";

function estimateExpandedHeight(reason: string): number {
  // Rough wrap estimate for the error column (~42 chars/line at this font size).
  const lines = Math.max(2, Math.ceil(reason.length / 42));
  return Math.min(220, 20 + lines * 18);
}

export default function VirtualizedImportIssuesTable({ issues }: { issues: ImportIssue[] }) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [expandedIndex, setExpandedIndex] = useState<number | null>(null);
  const virtualizer = useVirtualizer({
    count: issues.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) =>
      expandedIndex === index
        ? estimateExpandedHeight(issues[index]?.reason ?? "")
        : COLLAPSED_ROW_HEIGHT,
    overscan: 6,
  });
  const virtualRows = virtualizer.getVirtualItems();
  const viewportHeight = Math.min(issues.length, MAX_VISIBLE_ROWS) * COLLAPSED_ROW_HEIGHT;

  useEffect(() => {
    virtualizer.measure();
  }, [expandedIndex, issues, virtualizer]);

  const toggleRow = (index: number) => {
    setExpandedIndex((current) => (current === index ? null : index));
  };

  const onRowKeyDown = (event: KeyboardEvent<HTMLDivElement>, index: number) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      toggleRow(index);
    }
  };

  return (
    <div
      role="table"
      aria-label="Import errors"
      aria-rowcount={issues.length + 1}
      className="mt-2 w-full min-w-0 max-w-full overflow-hidden rounded-lg border border-border text-left text-[0.813rem]"
    >
      <div role="rowgroup" className="border-b border-border bg-elevated">
        <div role="row" aria-rowindex={1} className={`grid ${ISSUE_COLUMNS} text-muted`}>
          <div role="columnheader" className="min-w-0 px-3 py-2 font-medium">
            Parse File
          </div>
          <div role="columnheader" className="px-3 py-2 font-medium">
            Step
          </div>
          <div role="columnheader" className="min-w-0 px-3 py-2 font-medium">
            Error Message
          </div>
        </div>
      </div>
      <div
        ref={scrollRef}
        role="rowgroup"
        className="overflow-x-hidden overflow-y-auto outline-none"
        style={{ height: viewportHeight }}
      >
        <div className="relative w-full min-w-0" style={{ height: virtualizer.getTotalSize() }}>
          {virtualRows.map((virtualRow) => {
            const issue = issues[virtualRow.index];
            const expanded = expandedIndex === virtualRow.index;
            return (
              <div
                key={`${issue.kind}-${issue.step}-${issue.item}-${virtualRow.index}`}
                data-index={virtualRow.index}
                ref={virtualizer.measureElement}
                role="row"
                tabIndex={0}
                aria-rowindex={virtualRow.index + 2}
                aria-expanded={expanded}
                aria-label={`${expanded ? "Collapse" : "Expand"} error for ${issue.item}`}
                onClick={() => toggleRow(virtualRow.index)}
                onKeyDown={(event) => onRowKeyDown(event, virtualRow.index)}
                className={`absolute left-0 top-0 grid w-full min-w-0 cursor-pointer ${ISSUE_COLUMNS} items-start border-b border-border outline-none last:border-b-0 hover:bg-hover focus-visible:bg-hover focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent ${
                  expanded ? "bg-hover" : ""
                }`}
                style={{ transform: `translateY(${virtualRow.start}px)` }}
              >
                <div
                  role="cell"
                  title={issue.item}
                  className="min-w-0 overflow-hidden px-3 py-2 text-text"
                >
                  <span className="block truncate">{issue.item}</span>
                </div>
                <div role="cell" className="overflow-hidden px-3 py-2 capitalize text-text">
                  <span className="block truncate">{issue.step}</span>
                </div>
                <div
                  role="cell"
                  title={expanded ? undefined : issue.reason}
                  className="min-w-0 overflow-hidden px-3 py-2 text-text"
                >
                  <span
                    className={
                      expanded
                        ? "block whitespace-pre-wrap break-words"
                        : "line-clamp-2 break-words"
                    }
                  >
                    {issue.reason}
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}

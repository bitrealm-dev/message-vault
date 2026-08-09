"use client";

import type { YearThread } from "@/lib/types";
import { useDateTimeFormat } from "./useDateTimeFormat";

export function YearThreadPicker({
  years,
  activeYear,
  onSelect,
  emptyLabel,
  loading = false,
  showCounts = false,
}: {
  years: YearThread[];
  activeYear: number | null;
  onSelect: (year: YearThread) => void;
  emptyLabel: string;
  loading?: boolean;
  /** When true, show message counts next to each year (Unassigned). */
  showCounts?: boolean;
}) {
  const { formatDateRange } = useDateTimeFormat();
  return (
    <div>
      <h3 className="text-[15px] font-semibold text-text">
        Yearly messages
      </h3>
      {loading ? (
        <p className="mt-2 text-[12px] text-muted">Loading…</p>
      ) : years.length === 0 ? (
        <p className="mt-2 text-[12px] text-muted">{emptyLabel}</p>
      ) : (
        <div className="mt-2 flex flex-wrap items-center gap-y-1.5">
          {years.map((y, i) => {
            const active = activeYear === y.year;
            return (
              <span key={y.year} className="flex items-center">
                {i > 0 && (
                  <span className="mx-2 text-[13px] text-muted/50" aria-hidden>
                    |
                  </span>
                )}
                <button
                  type="button"
                  title={
                    showCounts
                      ? undefined
                      : `${y.messageCount} msgs · ${formatDateRange(y.dateStart, y.dateEnd)}`
                  }
                  onClick={() => onSelect(y)}
                  className={`text-[13px] font-medium ${
                    active ? "text-accent" : "text-text hover:text-accent"
                  }`}
                >
                  {y.year}
                  {showCounts && (
                    <span className="ml-2 text-muted">{y.messageCount}</span>
                  )}
                </button>
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
}

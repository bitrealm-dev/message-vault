"use client";

import type { SearchConversationHit } from "@/lib/search";
import { CountBadge } from "./CountBadge";
import { useDateTimeFormat } from "./useDateTimeFormat";

export function SearchResultsList({
  hits,
  total,
  loading,
  selectedConversationId,
  onSelect,
  emptyLabel = "No matches",
}: {
  hits: SearchConversationHit[];
  total: number;
  loading: boolean;
  selectedConversationId: number | null;
  onSelect: (hit: SearchConversationHit) => void;
  emptyLabel?: string;
}) {
  const { formatDateRange } = useDateTimeFormat();

  if (loading) {
    return (
      <p className="px-3 py-8 text-center text-[13px] text-muted">Searching…</p>
    );
  }

  if (hits.length === 0) {
    return (
      <p className="px-3 py-8 text-center text-[13px] text-muted">{emptyLabel}</p>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-gutter:stable]">
      <div className="sticky top-0 z-10 border-b border-border bg-sidebar px-3 py-1 text-[11px] font-semibold tracking-wider text-muted uppercase">
        Results · {total.toLocaleString()}
      </div>
      {hits.map((hit) => {
        const active = selectedConversationId === hit.conversationId;
        const dateLabel =
          hit.dateStart && hit.dateEnd
            ? formatDateRange(hit.dateStart, hit.dateEnd, " – ")
            : null;
        return (
          <button
            key={hit.conversationId}
            type="button"
            onClick={() => onSelect(hit)}
            className={`relative flex w-full flex-col gap-0.5 px-3 py-2 text-left transition-colors ${
              active
                ? "bg-accent/20 hover:bg-accent/25"
                : "hover:bg-hover"
            }`}
          >
            {active ? (
              <span
                aria-hidden
                className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
              />
            ) : null}
            <span className="flex min-w-0 items-center justify-between gap-2">
              <span className="min-w-0 truncate text-[13px] font-medium text-text">
                {hit.title}
              </span>
              <CountBadge
                count={hit.matchCount}
                title={`${hit.matchCount} matching messages`}
              />
            </span>
            {hit.topMatch?.snippet ? (
              <span className="line-clamp-2 text-[12px] text-muted">
                {hit.topMatch.snippet}
              </span>
            ) : null}
            <span className="flex min-w-0 items-center justify-between gap-2 text-[11px] text-muted">
              <span className="truncate capitalize">
                {hit.conversationType === "group" ? "Group" : "1-1"}
              </span>
              {dateLabel ? (
                <span className="tabular-nums">{dateLabel}</span>
              ) : null}
            </span>
          </button>
        );
      })}
    </div>
  );
}

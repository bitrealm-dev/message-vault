"use client";

import type { SearchConversationHit } from "@/lib/search";
import { CountBadge } from "./CountBadge";
import { useDateTimeFormat } from "./useDateTimeFormat";

const EMPTY_SELECTED_CONTACT_IDS: ReadonlySet<number> = new Set();

/** Title, match count, snippet, type and date range for one matching conversation. */
export function SearchHitSummary({ hit }: { hit: SearchConversationHit }) {
  const { formatDateRange } = useDateTimeFormat();
  const dateLabel =
    hit.dateStart && hit.dateEnd
      ? formatDateRange(hit.dateStart, hit.dateEnd, " – ")
      : null;

  return (
    <>
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
        {dateLabel ? <span className="tabular-nums">{dateLabel}</span> : null}
      </span>
    </>
  );
}

export function SearchResultsList({
  hits,
  total,
  loading,
  selectedConversationId,
  selectedContactIds = EMPTY_SELECTED_CONTACT_IDS,
  onToggleContact,
  onSelect,
  onContactContextMenu,
  emptyLabel = "No matches",
}: {
  hits: SearchConversationHit[];
  total: number;
  loading: boolean;
  selectedConversationId: number | null;
  selectedContactIds?: ReadonlySet<number>;
  onToggleContact?: (
    contactId: number,
    mods?: { shiftKey: boolean },
  ) => void;
  onSelect: (
    hit: SearchConversationHit,
    mods?: { shiftKey: boolean },
  ) => void;
  onContactContextMenu?: (contactId: number, x: number, y: number) => void;
  emptyLabel?: string;
}) {
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
        const checked =
          hit.contactId != null && selectedContactIds.has(hit.contactId);
        return (
          <div
            key={hit.conversationId}
            className={`relative flex w-full items-start transition-colors ${
              active || checked
                ? "bg-accent/20 hover:bg-accent/25"
                : "hover:bg-hover"
            }`}
            onContextMenu={(e) => {
              if (hit.contactId == null || !onContactContextMenu) return;
              e.preventDefault();
              onContactContextMenu(hit.contactId, e.clientX, e.clientY);
            }}
          >
            {active ? (
              <span
                aria-hidden
                className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
              />
            ) : null}
            {hit.contactId != null && onToggleContact ? (
              <label className="flex min-h-9 shrink-0 cursor-pointer items-start px-3 pt-2.5">
                <input
                  type="checkbox"
                  checked={checked}
                  aria-label={`Select ${hit.title}`}
                  onClick={(e) => {
                    e.preventDefault();
                    onToggleContact(hit.contactId!, { shiftKey: e.shiftKey });
                  }}
                  onChange={() => {}}
                  className="checkbox-list"
                />
              </label>
            ) : null}
            <button
              type="button"
              onClick={(e) => onSelect(hit, { shiftKey: e.shiftKey })}
              className={`flex min-w-0 flex-1 flex-col gap-0.5 py-2 text-left ${
                hit.contactId == null || !onToggleContact ? "px-3" : "pr-3"
              }`}
            >
              <SearchHitSummary hit={hit} />
            </button>
          </div>
        );
      })}
    </div>
  );
}

"use client";

import type { SearchConversationHit } from "@/lib/search";
import {
  searchResultKey,
  type SearchResultKey,
} from "@/lib/searchSelection";
import { CountBadge } from "./CountBadge";
import { highlightText } from "./highlightText";
import { GroupMessagesOutlineIcon, MessageIcon } from "./icons";
import { useDateTimeFormat } from "./useDateTimeFormat";

const EMPTY_SELECTED_CONTACT_IDS: ReadonlySet<number> = new Set();
const EMPTY_SELECTED_RESULT_KEYS: ReadonlySet<SearchResultKey> = new Set();
export type SearchSelectionModifiers = {
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
};

const EMPTY_TERMS: string[] = [];

/** "Show more" row for paginated search results. */
export function LoadMoreResultsRow({
  shown,
  total,
  loadingMore = false,
  onLoadMore,
}: {
  shown: number;
  total: number;
  loadingMore?: boolean;
  onLoadMore?: () => void;
}) {
  if (!onLoadMore || shown >= total) return null;
  return (
    <button
      type="button"
      disabled={loadingMore}
      onClick={onLoadMore}
      className="block w-full px-3 py-2.5 text-center text-[12px] font-medium text-accent transition-colors hover:bg-hover disabled:opacity-60"
    >
      {loadingMore
        ? "Loading…"
        : `Show more (${shown.toLocaleString()} of ${total.toLocaleString()})`}
    </button>
  );
}

/** Title, match count, snippet, type and date range for one matching conversation. */
export function SearchHitSummary({
  hit,
  highlightTerms = EMPTY_TERMS,
  showMeta = true,
}: {
  hit: SearchConversationHit;
  highlightTerms?: string[];
  /** Hide type/date line when nested under a contact (inspector covers it). */
  showMeta?: boolean;
}) {
  const { formatDateRange } = useDateTimeFormat();
  const dateLabel =
    hit.dateStart && hit.dateEnd
      ? formatDateRange(hit.dateStart, hit.dateEnd, " – ")
      : null;

  return (
    <>
      <span className="flex min-w-0 items-center justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5">
          {!showMeta ? (
            // Meta line is hidden when nested; keep the type readable via icon.
            hit.conversationType === "group" ? (
              <GroupMessagesOutlineIcon className="size-3.5 shrink-0 text-muted opacity-80" />
            ) : (
              <MessageIcon className="size-3.5 shrink-0 text-muted opacity-80" />
            )
          ) : null}
          <span className="min-w-0 truncate text-[13px] font-medium text-text">
            {hit.title}
          </span>
        </span>
        <CountBadge
          count={hit.matchCount}
          title={`${hit.matchCount} matching messages`}
        />
      </span>
      {hit.topMatch?.snippet ? (
        <span className="mt-0.5 line-clamp-2 text-[12px] text-muted">
          {highlightText(hit.topMatch.snippet, highlightTerms)}
        </span>
      ) : null}
      {showMeta ? (
        <span className="mt-1 flex min-w-0 items-center justify-between gap-6 text-[11px] text-muted">
          <span className="shrink-0 capitalize">
            {hit.conversationType === "group" ? "Group" : "Direct"}
          </span>
          {dateLabel ? (
            <span className="min-w-0 truncate text-right tabular-nums">
              {dateLabel}
            </span>
          ) : null}
        </span>
      ) : null}
    </>
  );
}

export function SearchResultsList({
  hits,
  total,
  loading,
  highlightTerms = EMPTY_TERMS,
  selectedConversationId,
  selectedContactIds = EMPTY_SELECTED_CONTACT_IDS,
  selectedResultKeys = EMPTY_SELECTED_RESULT_KEYS,
  onToggleContact,
  onToggleResult,
  onSelect,
  onContactContextMenu,
  onResultContextMenu,
  emptyLabel = "No matches",
  loadingMore = false,
  onLoadMore,
}: {
  hits: SearchConversationHit[];
  total: number;
  loading: boolean;
  highlightTerms?: string[];
  selectedConversationId: number | null;
  selectedContactIds?: ReadonlySet<number>;
  selectedResultKeys?: ReadonlySet<SearchResultKey>;
  onToggleContact?: (
    contactId: number,
    mods?: { shiftKey: boolean },
  ) => void;
  onToggleResult?: (
    hit: SearchConversationHit,
    mods: SearchSelectionModifiers,
  ) => void;
  onSelect: (
    hit: SearchConversationHit,
    mods?: SearchSelectionModifiers,
  ) => void;
  onContactContextMenu?: (contactId: number, x: number, y: number) => void;
  onResultContextMenu?: (
    hit: SearchConversationHit,
    x: number,
    y: number,
  ) => void;
  emptyLabel?: string;
  loadingMore?: boolean;
  onLoadMore?: () => void;
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
        const resultChecked = selectedResultKeys.has(searchResultKey(hit));
        const checked = onToggleResult
          ? resultChecked
          : hit.contactId != null && selectedContactIds.has(hit.contactId);
        return (
          <div
            key={hit.conversationId}
            className={`relative flex w-full items-start transition-colors ${
              active || checked
                ? "bg-accent/20 hover:bg-accent/25"
                : "hover:bg-hover"
            }`}
            onContextMenu={(e) => {
              if (onResultContextMenu) {
                e.preventDefault();
                onResultContextMenu(hit, e.clientX, e.clientY);
                return;
              }
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
            {onToggleResult || (hit.contactId != null && onToggleContact) ? (
              <label className="flex min-h-9 shrink-0 cursor-pointer items-start px-3 pt-2.5">
                <input
                  type="checkbox"
                  checked={checked}
                  aria-label={`Select ${hit.title}`}
                  onClick={(e) => {
                    e.preventDefault();
                    e.stopPropagation();
                    if (onToggleResult) {
                      onToggleResult(hit, {
                        shiftKey: e.shiftKey,
                        altKey: e.altKey,
                        metaKey: e.metaKey,
                        ctrlKey: e.ctrlKey,
                      });
                    } else {
                      onToggleContact?.(hit.contactId!, {
                        shiftKey: e.shiftKey,
                      });
                    }
                  }}
                  onChange={() => {}}
                  className="checkbox-list"
                />
              </label>
            ) : null}
            <button
              type="button"
              onClick={(e) =>
                onSelect(hit, {
                  shiftKey: e.shiftKey,
                  altKey: e.altKey,
                  metaKey: e.metaKey,
                  ctrlKey: e.ctrlKey,
                })
              }
              className={`flex min-w-0 flex-1 flex-col gap-0.5 py-2 text-left ${
                !onToggleResult && (hit.contactId == null || !onToggleContact)
                  ? "px-3"
                  : "pr-3"
              }`}
            >
              <SearchHitSummary hit={hit} highlightTerms={highlightTerms} />
            </button>
          </div>
        );
      })}
      <LoadMoreResultsRow
        shown={hits.length}
        total={total}
        loadingMore={loadingMore}
        onLoadMore={onLoadMore}
      />
    </div>
  );
}

"use client";

import type { SearchMessageHit } from "@/lib/search";
import { formatSizeBytes } from "@/lib/searchQuery";
import { highlightText } from "./highlightText";
import { GroupMessagesOutlineIcon, MessageIcon } from "./icons";
import { LoadMoreResultsRow } from "./SearchResultsList";
import { useDateTimeFormat } from "./useDateTimeFormat";

const EMPTY_TERMS: string[] = [];

function attachmentLabel(hit: SearchMessageHit): string | null {
  if (hit.attachments.length === 0) return null;
  const parts = hit.attachments.map((a) => {
    const name = a.name?.trim() || a.mimeType || "attachment";
    if (a.sizeBytes != null && a.sizeBytes > 0) {
      return `${name} (${formatSizeBytes(a.sizeBytes)})`;
    }
    return name;
  });
  if (parts.length <= 2) return parts.join(", ");
  return `${parts.slice(0, 2).join(", ")} +${parts.length - 2}`;
}

/** Flat message search results (`group:none`). */
export function SearchMessageResultsList({
  hits,
  total,
  loading,
  highlightTerms = EMPTY_TERMS,
  selectedMessageId,
  onSelect,
  emptyLabel = "No matches",
  loadingMore = false,
  onLoadMore,
}: {
  hits: SearchMessageHit[];
  total: number;
  loading: boolean;
  highlightTerms?: string[];
  selectedMessageId: number | null;
  onSelect: (hit: SearchMessageHit) => void;
  emptyLabel?: string;
  loadingMore?: boolean;
  onLoadMore?: () => void;
}) {
  const { formatDateTime } = useDateTimeFormat();

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
        Messages · {total.toLocaleString()}
      </div>
      {hits.map((hit) => {
        const active = selectedMessageId === hit.messageId;
        const att = attachmentLabel(hit);
        const senderLabel = hit.isFromMe
          ? "You"
          : hit.sender?.trim() || hit.title;
        return (
          <div
            key={hit.messageId}
            className={`relative flex w-full items-start transition-colors ${
              active ? "bg-accent/20 hover:bg-accent/25" : "hover:bg-hover"
            }`}
          >
            {active ? (
              <span
                aria-hidden
                className="absolute top-1 bottom-1 left-0 w-1 rounded-full bg-accent/80"
              />
            ) : null}
            <button
              type="button"
              onClick={() => onSelect(hit)}
              className="flex min-w-0 flex-1 flex-col gap-0.5 px-3 py-2 text-left"
            >
              <span className="flex min-w-0 items-center justify-between gap-2">
                <span className="flex min-w-0 items-center gap-1.5">
                  {hit.conversationType === "group" ? (
                    <GroupMessagesOutlineIcon className="size-3.5 shrink-0 text-muted opacity-80" />
                  ) : (
                    <MessageIcon className="size-3.5 shrink-0 text-muted opacity-80" />
                  )}
                  <span className="min-w-0 truncate text-[13px] font-medium text-text">
                    {hit.title}
                  </span>
                </span>
                <span className="shrink-0 text-[11px] tabular-nums text-muted">
                  {formatDateTime(hit.timestamp)}
                </span>
              </span>
              <span className="text-[11px] text-muted">{senderLabel}</span>
              {hit.contextBefore.length > 0 ? (
                <span className="mt-0.5 space-y-0.5 text-[11px] text-muted/80">
                  {hit.contextBefore.map((m) => (
                    <span key={m.id} className="block truncate">
                      {m.snippet || "…"}
                    </span>
                  ))}
                </span>
              ) : null}
              {hit.snippet ? (
                <span className="mt-0.5 line-clamp-2 text-[12px] text-muted">
                  {highlightText(hit.snippet, highlightTerms)}
                </span>
              ) : null}
              {hit.contextAfter.length > 0 ? (
                <span className="mt-0.5 space-y-0.5 text-[11px] text-muted/80">
                  {hit.contextAfter.map((m) => (
                    <span key={m.id} className="block truncate">
                      {m.snippet || "…"}
                    </span>
                  ))}
                </span>
              ) : null}
              {att ? (
                <span className="mt-0.5 truncate text-[11px] text-muted">
                  {att}
                </span>
              ) : null}
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

import { useState, useEffect, useCallback } from "react";
import { apiClient } from "../lib/api";
import type { Conversation } from "../lib/types";
import ConversationRow from "../components/ConversationRow";
import VirtualList, { type VisibleRange } from "../components/VirtualList";
import {
  formatVisibleRange,
  usePagedList,
  type PagedFetchPage,
} from "../lib/usePagedList";

const QUERY_DEBOUNCE_MS = 300;

type ConversationsPage = {
  conversations: Conversation[];
  total: number;
  limit: number;
  offset: number;
};

export default function ConversationList({
  selectedId,
  onSelect,
  query,
}: {
  selectedId: string | null;
  onSelect: (conversation: Conversation) => void;
  query: string;
}) {
  const [debouncedQ, setDebouncedQ] = useState(query);
  const [visibleRange, setVisibleRange] = useState<VisibleRange>({ start: 0, end: 0 });

  useEffect(() => {
    // Structured filters should apply immediately (no debounce flicker/empty wait).
    if (/\b(contact:|handle:|import:|is:direct|is:group|is:trash|participants:)\b/i.test(query)) {
      setDebouncedQ(query);
      return;
    }
    const t = window.setTimeout(() => setDebouncedQ(query), QUERY_DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [query]);

  const fetchPage = useCallback<PagedFetchPage<Conversation>>(
    async ({ limit, offset, signal }) => {
      const params = new URLSearchParams({
        q: debouncedQ,
        limit: String(limit),
        offset: String(offset),
      });
      const res = await apiClient.get<ConversationsPage>(
        `/v1/export/conversations?${params}`,
        { signal },
      );
      return {
        items: res.conversations || [],
        total: res.total ?? 0,
      };
    },
    [debouncedQ],
  );

  const {
    items: conversations,
    total,
    loading,
    refreshing,
    filling,
    error,
    hasMore,
    loadMore,
  } = usePagedList(debouncedQ, fetchPage);

  const rangeLabel =
    loading && conversations.length === 0
      ? "Loading…"
      : formatVisibleRange(
          visibleRange.start,
          visibleRange.end,
          total,
          conversations.length,
        );

  let activitySuffix = "";
  if (refreshing) activitySuffix = " · updating…";
  else if (filling) activitySuffix = " · loading more…";

  if (error && conversations.length === 0) {
    return (
      <div className="p-4 text-[0.813rem] text-danger">
        Could not load conversations: {error}
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="shrink-0 border-b border-border px-3 py-1.5 text-[0.688rem] text-muted">
        {rangeLabel}
        {activitySuffix}
      </div>
      <VirtualList
        count={conversations.length}
        estimateSize={64}
        dynamicSize
        onVisibleRangeChange={setVisibleRange}
        onNearEnd={() => {
          if (hasMore) loadMore();
        }}
        empty={
          !loading ? (
            <div className="p-4 text-[0.813rem] text-muted">
              No conversations
            </div>
          ) : null
        }
        renderItem={(index) => {
          const c = conversations[index];
          if (!c) return null;
          return (
            <ConversationRow
              conversation={c}
              isSelected={c.id === selectedId}
              onClick={() => onSelect(c)}
            />
          );
        }}
      />
    </div>
  );
}

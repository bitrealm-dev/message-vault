import { useState, useEffect, useCallback, useRef } from "react";
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

export type ConversationAutoSelect = "first" | "sole";

export default function ConversationList({
  selectedId,
  onSelect,
  query,
  autoSelect = null,
  onAutoSelectDone,
}: {
  selectedId: string | null;
  onSelect: (conversation: Conversation) => void;
  query: string;
  autoSelect?: ConversationAutoSelect | null;
  onAutoSelectDone?: () => void;
}) {
  const [debouncedQ, setDebouncedQ] = useState(query);
  const [visibleRange, setVisibleRange] = useState<VisibleRange>({ start: 0, end: 0 });
  const autoSelectRef = useRef(autoSelect);
  autoSelectRef.current = autoSelect;
  const onAutoSelectDoneRef = useRef(onAutoSelectDone);
  onAutoSelectDoneRef.current = onAutoSelectDone;
  const didAutoSelectRef = useRef(false);

  useEffect(() => {
    didAutoSelectRef.current = false;
  }, [autoSelect, query]);

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

  useEffect(() => {
    if (loading || didAutoSelectRef.current) return;
    const mode = autoSelectRef.current;
    if (!mode) return;

    if (mode === "first" && conversations[0]) {
      didAutoSelectRef.current = true;
      onSelect(conversations[0]);
      onAutoSelectDoneRef.current?.();
      return;
    }
    if (mode === "sole" && conversations.length === 1 && total === 1) {
      didAutoSelectRef.current = true;
      onSelect(conversations[0]);
      onAutoSelectDoneRef.current?.();
      return;
    }
    if (mode === "sole" && !loading && (conversations.length === 0 || total !== 1)) {
      didAutoSelectRef.current = true;
      onAutoSelectDoneRef.current?.();
    }
    if (mode === "first" && !loading && conversations.length === 0) {
      didAutoSelectRef.current = true;
      onAutoSelectDoneRef.current?.();
    }
  }, [loading, conversations, total, onSelect]);

  const rangeLabel =
    loading && conversations.length === 0
      ? "Loading…"
      : formatVisibleRange(
          visibleRange.start,
          visibleRange.end,
          total,
          conversations.length,
        );

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
        {refreshing ? " · updating…" : filling ? " · loading more…" : null}
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

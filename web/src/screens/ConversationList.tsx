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

  const { items: conversations, total, loading, refreshing, filling, error } = usePagedList(
    debouncedQ,
    fetchPage,
  );

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
      <div style={{ padding: "1rem", fontSize: "0.813rem", color: "var(--danger)" }}>
        Could not load conversations: {error}
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}>
      <div
        style={{
          padding: "0.375rem 0.75rem",
          fontSize: "0.688rem",
          color: "var(--muted)",
          borderBottom: "1px solid var(--border)",
          flexShrink: 0,
        }}
      >
        {rangeLabel}
        {refreshing ? " · updating…" : filling ? " · loading…" : null}
      </div>
      <VirtualList
        count={conversations.length}
        estimateSize={64}
        onVisibleRangeChange={setVisibleRange}
        empty={
          !loading ? (
            <div style={{ padding: "1rem", fontSize: "0.813rem", color: "var(--muted)" }}>
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

import { useState, useEffect, useRef, useCallback } from "react";
import { apiClient } from "../lib/api";
import type { Conversation } from "../lib/types";
import ConversationRow from "../components/ConversationRow";

const PAGE_SIZE = 40;
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
  const [conversations, setConversations] = useState<Conversation[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState("");
  const offsetRef = useRef(0);
  const totalRef = useRef(0);
  const loadingMoreRef = useRef(false);
  const hasLoadedRef = useRef(false);
  const sessionSignalRef = useRef<AbortSignal | null>(null);
  const sentinelRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const t = window.setTimeout(() => setDebouncedQ(query), QUERY_DEBOUNCE_MS);
    return () => window.clearTimeout(t);
  }, [query]);

  const loadPage = useCallback(
    async (offset: number, append: boolean, signal: AbortSignal) => {
      if (append) {
        if (loadingMoreRef.current) return;
        loadingMoreRef.current = true;
        setLoadingMore(true);
      } else if (hasLoadedRef.current) {
        setError("");
      } else {
        setLoading(true);
        setError("");
      }

      try {
        const params = new URLSearchParams({
          q: debouncedQ,
          limit: String(PAGE_SIZE),
          offset: String(offset),
        });
        const res = await apiClient.get<ConversationsPage>(
          `/v1/export/conversations?${params}`,
          { signal },
        );
        if (signal.aborted) return;
        const items = res.conversations || [];
        setTotal(res.total ?? 0);
        totalRef.current = res.total ?? 0;
        offsetRef.current = offset + items.length;
        setConversations((prev) => (append ? [...prev, ...items] : items));
        hasLoadedRef.current = true;
      } catch (e) {
        if (signal.aborted) return;
        if (!append) {
          setConversations([]);
          setTotal(0);
          totalRef.current = 0;
          offsetRef.current = 0;
          setError(e instanceof Error ? e.message : String(e));
        }
      } finally {
        if (append) {
          loadingMoreRef.current = false;
          if (!signal.aborted) setLoadingMore(false);
        } else if (!signal.aborted) {
          setLoading(false);
        }
      }
    },
    [debouncedQ],
  );

  useEffect(() => {
    const ac = new AbortController();
    sessionSignalRef.current = ac.signal;
    offsetRef.current = 0;
    loadingMoreRef.current = false;
    // Keep prior rows until the new page arrives (avoids keypress flicker).
    void loadPage(0, false, ac.signal);
    return () => ac.abort();
  }, [loadPage]);

  useEffect(() => {
    const el = sentinelRef.current;
    if (!el) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries[0]?.isIntersecting) return;
        if (loading || loadingMoreRef.current) return;
        if (offsetRef.current >= totalRef.current) return;
        const signal = sessionSignalRef.current;
        if (!signal || signal.aborted) return;
        void loadPage(offsetRef.current, true, signal);
      },
      { root: el.parentElement, rootMargin: "80px" },
    );
    observer.observe(el);
    return () => observer.disconnect();
  }, [loadPage, loading, conversations.length, total]);

  const rangeLabel =
    total === 0
      ? "0 of 0"
      : `${conversations.length === 0 ? 0 : 1}–${conversations.length} of ${total}`;

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
        {loading && conversations.length === 0 ? "Loading…" : rangeLabel}
      </div>
      <div style={{ overflow: "auto", flex: 1, minHeight: 0 }}>
        {!loading && conversations.length === 0 ? (
          <div style={{ padding: "1rem", fontSize: "0.813rem", color: "var(--muted)" }}>
            No conversations
          </div>
        ) : (
          <>
            {conversations.map((c) => (
              <ConversationRow
                key={c.id}
                conversation={c}
                isSelected={c.id === selectedId}
                onClick={() => onSelect(c)}
              />
            ))}
            <div ref={sentinelRef} style={{ height: 1 }} />
            {loadingMore && (
              <div style={{ padding: "0.75rem", fontSize: "0.75rem", color: "var(--muted)", textAlign: "center" }}>
                Loading more…
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

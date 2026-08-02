"use client";

import type {
  SearchContactHit,
  SearchConversationHit,
  SearchResult,
} from "@/lib/search";
import { parseSearchQuery } from "@/lib/searchQuery";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

const DEBOUNCE_MS = 250;

export function useVaultSearch(initialQuery = "") {
  const [draft, setDraft] = useState(initialQuery);
  const [committed, setCommitted] = useState(initialQuery.trim());
  const [hits, setHits] = useState<SearchConversationHit[]>([]);
  const [contactHits, setContactHits] = useState<SearchContactHit[]>([]);
  const [contactIds, setContactIds] = useState<number[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshToken, setRefreshToken] = useState(0);
  const seqRef = useRef(0);

  const resultsMode = committed.length > 0;

  const parsed = useMemo(() => parseSearchQuery(committed), [committed]);
  const mode = parsed.mode;

  const highlightTerms = useMemo(
    () => [
      ...parsed.terms,
      ...parsed.phrases,
      ...(parsed.subject ? [parsed.subject] : []),
    ],
    [parsed],
  );

  const submit = useCallback((q: string) => {
    setDraft(q);
    setCommitted(q.trim());
  }, []);
  const refresh = useCallback(() => {
    setRefreshToken((value) => value + 1);
  }, []);

  useEffect(() => {
    const clear = () => {
      setHits([]);
      setContactHits([]);
      setContactIds([]);
      setTotal(0);
    };
    if (!committed) {
      clear();
      setLoading(false);
      setError(null);
      return;
    }
    const timer = window.setTimeout(() => {
      const seq = ++seqRef.current;
      setLoading(true);
      setLoadingMore(false);
      setError(null);
      void fetch(`/api/search?q=${encodeURIComponent(committed)}`)
        .then(async (res) => {
          const json = (await res.json()) as SearchResult & { error?: string };
          if (seq !== seqRef.current) return;
          if (!res.ok) {
            setError(json.error ?? "Search failed");
            clear();
            return;
          }
          setHits(json.hits ?? []);
          setContactHits(json.contacts ?? []);
          setContactIds(json.contactIds ?? []);
          setTotal(
            json.contacts
              ? (json.totalContacts ?? 0)
              : (json.totalConversations ?? 0),
          );
        })
        .catch((err: unknown) => {
          if (seq !== seqRef.current) return;
          setError(err instanceof Error ? err.message : "Search failed");
          clear();
        })
        .finally(() => {
          if (seq === seqRef.current) setLoading(false);
        });
    }, DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [committed, refreshToken]);

  const loadedCount = mode === "contacts" ? contactHits.length : hits.length;
  const hasMore = resultsMode && !loading && loadedCount > 0 && loadedCount < total;

  /** Fetch the next page and append it (People pages contacts, Messages pages conversations). */
  const loadMore = useCallback(() => {
    if (!committed || loading || loadingMore) return;
    const seq = seqRef.current;
    const isContacts = mode === "contacts";
    const offset = isContacts ? contactHits.length : hits.length;
    setLoadingMore(true);
    void fetch(
      `/api/search?q=${encodeURIComponent(committed)}&offset=${offset}`,
    )
      .then(async (res) => {
        const json = (await res.json()) as SearchResult & { error?: string };
        if (seq !== seqRef.current) return;
        if (!res.ok) {
          setError(json.error ?? "Search failed");
          return;
        }
        setContactIds(json.contactIds ?? []);
        setTotal(
          json.contacts
            ? (json.totalContacts ?? 0)
            : (json.totalConversations ?? 0),
        );
        if (isContacts) {
          setContactHits((prev) => {
            const seen = new Set(prev.map((hit) => hit.contact.id));
            const next = (json.contacts ?? []).filter(
              (hit) => !seen.has(hit.contact.id),
            );
            return next.length > 0 ? [...prev, ...next] : prev;
          });
        } else {
          setHits((prev) => {
            const seen = new Set(prev.map((hit) => hit.conversationId));
            const next = (json.hits ?? []).filter(
              (hit) => !seen.has(hit.conversationId),
            );
            return next.length > 0 ? [...prev, ...next] : prev;
          });
        }
      })
      .catch((err: unknown) => {
        if (seq !== seqRef.current) return;
        setError(err instanceof Error ? err.message : "Search failed");
      })
      .finally(() => {
        if (seq === seqRef.current) setLoadingMore(false);
      });
  }, [committed, loading, loadingMore, mode, contactHits.length, hits.length]);

  return {
    draft,
    setDraft,
    committed,
    submit,
    resultsMode,
    mode,
    hits,
    contactHits,
    contactIds,
    total,
    loading,
    loadingMore,
    hasMore,
    loadMore,
    error,
    highlightTerms,
    refresh,
  };
}

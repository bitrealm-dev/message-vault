"use client";

import type { SearchConversationHit, SearchResult } from "@/lib/search";
import { parseSearchQuery } from "@/lib/searchQuery";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

const DEBOUNCE_MS = 250;

export function useVaultSearch(initialQuery = "") {
  const [draft, setDraft] = useState(initialQuery);
  const [committed, setCommitted] = useState(initialQuery.trim());
  const [hits, setHits] = useState<SearchConversationHit[]>([]);
  const [contactIds, setContactIds] = useState<number[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const seqRef = useRef(0);

  const resultsMode = committed.length > 0;

  const highlightTerms = useMemo(() => {
    const parsed = parseSearchQuery(committed);
    return [
      ...parsed.terms,
      ...parsed.phrases,
      ...(parsed.subject ? [parsed.subject] : []),
    ];
  }, [committed]);

  const submit = useCallback((q: string) => {
    setDraft(q);
    setCommitted(q.trim());
  }, []);

  useEffect(() => {
    if (!committed) {
      setHits([]);
      setContactIds([]);
      setTotal(0);
      setLoading(false);
      setError(null);
      return;
    }
    const timer = window.setTimeout(() => {
      const seq = ++seqRef.current;
      setLoading(true);
      setError(null);
      void fetch(`/api/search?q=${encodeURIComponent(committed)}`)
        .then(async (res) => {
          const json = (await res.json()) as SearchResult & { error?: string };
          if (seq !== seqRef.current) return;
          if (!res.ok) {
            setError(json.error ?? "Search failed");
            setHits([]);
            setContactIds([]);
            setTotal(0);
            return;
          }
          setHits(json.hits ?? []);
          setContactIds(json.contactIds ?? []);
          setTotal(json.totalConversations ?? 0);
        })
        .catch((err: unknown) => {
          if (seq !== seqRef.current) return;
          setError(err instanceof Error ? err.message : "Search failed");
          setHits([]);
          setContactIds([]);
          setTotal(0);
        })
        .finally(() => {
          if (seq === seqRef.current) setLoading(false);
        });
    }, DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [committed]);

  return {
    draft,
    setDraft,
    committed,
    submit,
    resultsMode,
    hits,
    contactIds,
    total,
    loading,
    error,
    highlightTerms,
  };
}

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
  const [error, setError] = useState<string | null>(null);
  const seqRef = useRef(0);

  const resultsMode = committed.length > 0;

  const parsed = useMemo(() => parseSearchQuery(committed), [committed]);
  const showContact = parsed.showContact;

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
  }, [committed]);

  return {
    draft,
    setDraft,
    committed,
    submit,
    resultsMode,
    showContact,
    hits,
    contactHits,
    contactIds,
    total,
    loading,
    error,
    highlightTerms,
  };
}

"use client";

import { parseSearchQuery } from "@/lib/searchQuery";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

const DEBOUNCE_MS = 250;

export type ThreadFindMatch = { id: number; timestamp: string };

export type ThreadFind = {
  open: boolean;
  query: string;
  setQuery: (q: string) => void;
  /** Matching message ids in the open thread, oldest first. */
  matches: ThreadFindMatch[];
  /** Position within `matches` of the match currently jumped to. */
  index: number;
  loading: boolean;
  /** Words / phrases to highlight in message bodies. */
  terms: string[];
  openBar: () => void;
  /** Open prefilled (global search handoff), positioned at `seedMessageId`. */
  openWith: (query: string, seedMessageId?: number | null) => void;
  close: () => void;
  /** Jump to the next (newer) match, wrapping around. */
  next: () => void;
  /** Jump to the previous (older) match, wrapping around. */
  prev: () => void;
};

/**
 * In-thread find: fetches every matching message id for the open conversation
 * and steps through them. Reused by both global-search handoff and manual
 * Ctrl+F style find.
 */
export function useThreadFind({
  conversationIds,
  source,
  onJump,
}: {
  conversationIds: number[] | null;
  source: string | null;
  /** Scroll the thread to a match, loading its page first if needed. */
  onJump: (match: ThreadFindMatch) => void;
}): ThreadFind {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [matches, setMatches] = useState<ThreadFindMatch[]>([]);
  const [index, setIndex] = useState(0);
  const [loading, setLoading] = useState(false);
  const seqRef = useRef(0);
  const seedRef = useRef<number | null>(null);
  const onJumpRef = useRef(onJump);
  onJumpRef.current = onJump;

  const convKey =
    conversationIds && conversationIds.length > 0
      ? conversationIds.join(",")
      : "";

  const terms = useMemo(() => {
    const parsed = parseSearchQuery(query);
    return [
      ...parsed.terms,
      ...parsed.phrases,
      ...(parsed.subject ? [parsed.subject] : []),
    ];
  }, [query]);

  useEffect(() => {
    if (!open || !convKey || !query.trim()) {
      seqRef.current += 1;
      setMatches([]);
      setIndex(0);
      setLoading(false);
      return;
    }
    const timer = window.setTimeout(() => {
      const seq = ++seqRef.current;
      setLoading(true);
      const sourcePart = source
        ? `&source=${encodeURIComponent(source)}`
        : "";
      void fetch(
        `/api/search?conv=${convKey}&q=${encodeURIComponent(query)}${sourcePart}`,
      )
        .then(async (res) => {
          const json = (await res.json()) as {
            matches?: ThreadFindMatch[];
            error?: string;
          };
          if (seq !== seqRef.current) return;
          const list = res.ok ? (json.matches ?? []) : [];
          setMatches(list);
          if (list.length === 0) {
            setIndex(0);
            return;
          }
          // Start at the newest match (threads load newest page first),
          // unless a global search hit seeded a specific message.
          let at = list.length - 1;
          const seed = seedRef.current;
          if (seed != null) {
            const i = list.findIndex((m) => m.id === seed);
            if (i >= 0) at = i;
          }
          seedRef.current = null;
          setIndex(at);
          onJumpRef.current(list[at]!);
        })
        .catch(() => {
          if (seq === seqRef.current) setMatches([]);
        })
        .finally(() => {
          if (seq === seqRef.current) setLoading(false);
        });
    }, DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [open, convKey, query, source]);

  const goTo = useCallback(
    (i: number) => {
      if (matches.length === 0) return;
      const wrapped = ((i % matches.length) + matches.length) % matches.length;
      setIndex(wrapped);
      onJumpRef.current(matches[wrapped]!);
    },
    [matches],
  );
  const next = useCallback(() => goTo(index + 1), [goTo, index]);
  const prev = useCallback(() => goTo(index - 1), [goTo, index]);

  const openBar = useCallback(() => setOpen(true), []);
  const openWith = useCallback(
    (q: string, seedMessageId?: number | null) => {
      seedRef.current = seedMessageId ?? null;
      setQuery(q);
      setOpen(true);
    },
    [],
  );
  const close = useCallback(() => setOpen(false), []);

  return {
    open,
    query,
    setQuery,
    matches,
    index,
    loading,
    terms,
    openBar,
    openWith,
    close,
    next,
    prev,
  };
}

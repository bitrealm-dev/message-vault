"use client";

import {
  mergeMessagePages,
  messagesCoverIds,
} from "@/lib/messageCursor";
import { DEFAULT_MESSAGE_PAGE_SIZE } from "@/lib/messagePageSize";
import type { MessageRow } from "@/lib/types";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { fetchThreadMessages } from "./useThreadMessages";

type PageCacheEntry = {
  messages: MessageRow[];
  nextOlderCursor: string | null;
  hasOlder: boolean;
};

function cacheKey(ids: number[], sourceQuery: string): string {
  return `${ids.join(",")}|${sourceQuery}`;
}

async function fetchMessagePage(
  conversationIds: number[],
  sourceQuery: string,
  before: string | null,
  limit = DEFAULT_MESSAGE_PAGE_SIZE,
): Promise<PageCacheEntry> {
  const ids = conversationIds.join(",");
  const beforePart = before ? `&before=${encodeURIComponent(before)}` : "";
  const res = await fetch(
    `/api/messages?conversationIds=${ids}&page=1&limit=${limit}${beforePart}${sourceQuery}`,
  );
  const data = (await res.json()) as {
    error?: string;
    messages?: MessageRow[];
    nextOlderCursor?: string | null;
    hasOlder?: boolean;
  };
  if (!res.ok) {
    throw new Error(data.error ?? "Failed to load messages");
  }
  return {
    messages: data.messages ?? [],
    nextOlderCursor: data.nextOlderCursor ?? null,
    hasOlder: Boolean(data.hasOlder),
  };
}

/**
 * Progressive newest-first message loading for contact browse.
 * Caches pages per conversationIds+source and can merge year-scoped loads.
 */
export function useProgressiveThreadMessages(options: {
  conversationIds: number[] | null;
  sourceQuery: string;
  enabled?: boolean;
  reloadToken?: number | string;
}) {
  const {
    conversationIds,
    sourceQuery,
    enabled = true,
    reloadToken = 0,
  } = options;
  const cacheRef = useRef(new Map<string, PageCacheEntry>());
  const [messages, setMessages] = useState<MessageRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [hasOlder, setHasOlder] = useState(false);
  const seqRef = useRef(0);
  const loadingOlderRef = useRef(false);
  const prevReloadRef = useRef(reloadToken);
  const idsKey = conversationIds?.join(",") ?? "";
  const canFetch =
    enabled && conversationIds != null && conversationIds.length > 0;

  const applyEntry = useCallback((entry: PageCacheEntry) => {
    setMessages(entry.messages);
    setHasOlder(entry.hasOlder);
  }, []);

  const invalidate = useCallback((ids?: number[] | null) => {
    if (ids == null) {
      cacheRef.current.clear();
      return;
    }
    const prefix = `${ids.join(",")}|`;
    for (const key of [...cacheRef.current.keys()]) {
      if (key.startsWith(prefix)) cacheRef.current.delete(key);
    }
  }, []);

  useEffect(() => {
    if (prevReloadRef.current !== reloadToken) {
      prevReloadRef.current = reloadToken;
      if (conversationIds != null) invalidate(conversationIds);
      else invalidate();
    }

    if (!canFetch || conversationIds == null) {
      // Mirror useThreadMessages: clear local message state when the thread closes.
      /* eslint-disable react-hooks/set-state-in-effect -- reset on closed thread */
      setMessages([]);
      setHasOlder(false);
      setLoading(false);
      setLoadingOlder(false);
      /* eslint-enable react-hooks/set-state-in-effect */
      loadingOlderRef.current = false;
      return;
    }

    const key = cacheKey(conversationIds, sourceQuery);
    const cached = cacheRef.current.get(key);
    if (cached) {
      applyEntry(cached);
      setLoading(false);
      return;
    }

    const seq = ++seqRef.current;
    let cancelled = false;
    setLoading(true);
    setLoadingOlder(false);
    loadingOlderRef.current = false;
    fetchMessagePage(conversationIds, sourceQuery, null)
      .then((page) => {
        if (cancelled || seq !== seqRef.current) return;
        cacheRef.current.set(key, page);
        applyEntry(page);
      })
      .catch(() => {
        if (cancelled || seq !== seqRef.current) return;
        setMessages([]);
        setHasOlder(false);
      })
      .finally(() => {
        if (!cancelled && seq === seqRef.current) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [
    canFetch,
    idsKey,
    sourceQuery,
    reloadToken,
    conversationIds,
    applyEntry,
    invalidate,
  ]);

  const loadOlder = useCallback(async () => {
    if (!canFetch || conversationIds == null) return;
    const key = cacheKey(conversationIds, sourceQuery);
    const current = cacheRef.current.get(key);
    if (
      !current?.hasOlder ||
      !current.nextOlderCursor ||
      loadingOlderRef.current
    ) {
      return;
    }
    loadingOlderRef.current = true;
    const seq = ++seqRef.current;
    setLoadingOlder(true);
    try {
      const page = await fetchMessagePage(
        conversationIds,
        sourceQuery,
        current.nextOlderCursor,
      );
      if (seq !== seqRef.current) return;
      const latest = cacheRef.current.get(key) ?? current;
      const entry: PageCacheEntry = {
        messages: mergeMessagePages(latest.messages, page.messages),
        nextOlderCursor: page.nextOlderCursor,
        hasOlder: page.hasOlder,
      };
      cacheRef.current.set(key, entry);
      applyEntry(entry);
    } finally {
      if (seq === seqRef.current) {
        setLoadingOlder(false);
        loadingOlderRef.current = false;
      }
    }
  }, [canFetch, conversationIds, sourceQuery, applyEntry]);

  const ensureYearLoaded = useCallback(
    async (year: number): Promise<boolean> => {
      if (!canFetch || conversationIds == null) return false;
      const key = cacheKey(conversationIds, sourceQuery);
      const current = cacheRef.current.get(key);
      if (
        current &&
        current.messages.some((m) => m.timestamp.startsWith(`${year}-`)) &&
        !current.hasOlder
      ) {
        return true;
      }
      const seq = ++seqRef.current;
      try {
        const yearMessages = await fetchThreadMessages(
          conversationIds,
          year,
          sourceQuery,
        );
        if (seq !== seqRef.current) return false;
        const latest = cacheRef.current.get(key);
        const entry: PageCacheEntry = {
          messages: mergeMessagePages(latest?.messages ?? [], yearMessages),
          nextOlderCursor: latest?.nextOlderCursor ?? null,
          hasOlder: latest?.hasOlder ?? false,
        };
        cacheRef.current.set(key, entry);
        applyEntry(entry);
        return yearMessages.length > 0 || Boolean(latest?.messages.length);
      } catch {
        return false;
      }
    },
    [canFetch, conversationIds, sourceQuery, applyEntry],
  );

  const ensureMessageIdsLoaded = useCallback(
    async (ids: number[], yearHint?: number | null): Promise<boolean> => {
      if (!canFetch || conversationIds == null) return false;
      const key = cacheKey(conversationIds, sourceQuery);
      const current = cacheRef.current.get(key)?.messages ?? messages;
      if (messagesCoverIds(current, ids)) return true;
      if (yearHint != null) {
        await ensureYearLoaded(yearHint);
        const afterYear = cacheRef.current.get(key)?.messages ?? [];
        if (messagesCoverIds(afterYear, ids)) return true;
      }
      let guard = 0;
      while (guard < 40) {
        guard += 1;
        const entry = cacheRef.current.get(key);
        if (!entry) break;
        if (messagesCoverIds(entry.messages, ids)) return true;
        if (!entry.hasOlder || !entry.nextOlderCursor) break;
        await loadOlder();
      }
      return messagesCoverIds(
        cacheRef.current.get(key)?.messages ?? [],
        ids,
      );
    },
    [
      canFetch,
      conversationIds,
      sourceQuery,
      messages,
      ensureYearLoaded,
      loadOlder,
    ],
  );

  return {
    messages,
    loading,
    loadingOlder,
    hasOlder,
    loadOlder,
    ensureYearLoaded,
    ensureMessageIdsLoaded,
    invalidate,
    setMessages,
  };
}

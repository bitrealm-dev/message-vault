"use client";

import type {
  ContactDetail,
  GroupChatThread,
  YearThread,
} from "@/lib/types";
import { useCallback, useEffect, useRef, useState } from "react";

export type ContactThreadBundle = {
  detail: ContactDetail;
  yearly: YearThread[];
  groupChats: GroupChatThread[];
  messageSources: string[];
  sourceCounts: { all: number; bySource: Record<string, number> };
};

function cacheKey(contactId: number, sourceQuery: string): string {
  return `${contactId}|${sourceQuery}`;
}

/**
 * Lazy-loads and caches per-contact thread bundles for the merged people tree.
 * Fetch happens on expansion; results are reused until invalidate/reload.
 */
export function useBrowsePeopleTree({
  sourceQuery,
  reloadToken = 0,
}: {
  sourceQuery: string;
  reloadToken?: number;
}) {
  const cacheRef = useRef(new Map<string, ContactThreadBundle>());
  const [expandedContactId, setExpandedContactId] = useState<number | null>(
    null,
  );
  const [bundle, setBundle] = useState<ContactThreadBundle | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const seqRef = useRef(0);
  const prevReloadTokenRef = useRef(reloadToken);

  const applyBundle = useCallback((next: ContactThreadBundle | null) => {
    setBundle(next);
  }, []);

  const loadContact = useCallback(
    async (contactId: number, options?: { force?: boolean }) => {
      const key = cacheKey(contactId, sourceQuery);
      if (!options?.force) {
        const cached = cacheRef.current.get(key);
        if (cached) {
          applyBundle(cached);
          setError(null);
          setLoading(false);
          return cached;
        }
      }
      const seq = ++seqRef.current;
      setLoading(true);
      setError(null);
      try {
        const res = await fetch(
          `/api/contacts/${contactId}/threads${
            sourceQuery ? `?${sourceQuery.slice(1)}` : ""
          }`,
        );
        const data = (await res.json()) as {
          error?: string;
          contact?: ContactDetail;
          yearly?: YearThread[];
          groupChats?: GroupChatThread[];
          messageSources?: string[];
          sourceCounts?: { all: number; bySource: Record<string, number> };
        };
        if (seq !== seqRef.current) return null;
        if (!res.ok || data.error || !data.contact) {
          setError(data.error ?? "Failed to load threads");
          applyBundle(null);
          return null;
        }
        const next: ContactThreadBundle = {
          detail: data.contact,
          yearly: data.yearly ?? [],
          groupChats: data.groupChats ?? [],
          messageSources: data.messageSources ?? [],
          sourceCounts: data.sourceCounts ?? { all: 0, bySource: {} },
        };
        cacheRef.current.set(key, next);
        applyBundle(next);
        return next;
      } catch (err) {
        if (seq !== seqRef.current) return null;
        setError(err instanceof Error ? err.message : "Failed to load threads");
        applyBundle(null);
        return null;
      } finally {
        if (seq === seqRef.current) setLoading(false);
      }
    },
    [applyBundle, sourceQuery],
  );

  const expandContact = useCallback(
    (contactId: number | null, options?: { force?: boolean }) => {
      setExpandedContactId(contactId);
      if (contactId == null) {
        applyBundle(null);
        setLoading(false);
        setError(null);
        return;
      }
      // Drop the previous contact's bundle immediately so consumers never apply
      // stale detail/threads while the next fetch (or cache read) is in flight.
      setBundle((prev) => {
        if (prev?.detail.id === contactId && !options?.force) return prev;
        return null;
      });
      setLoading(true);
      setError(null);
      void loadContact(contactId, options);
    },
    [loadContact],
  );

  const invalidate = useCallback(() => {
    cacheRef.current.clear();
    if (expandedContactId != null) {
      void loadContact(expandedContactId, { force: true });
    }
  }, [expandedContactId, loadContact]);

  const patchCachedDetail = useCallback(
    (contactId: number, patch: Partial<ContactDetail>) => {
      for (const [key, value] of cacheRef.current) {
        if (!key.startsWith(`${contactId}|`)) continue;
        cacheRef.current.set(key, {
          ...value,
          detail: { ...value.detail, ...patch },
        });
      }
      setBundle((prev) => {
        if (!prev || expandedContactId !== contactId) return prev;
        return { ...prev, detail: { ...prev.detail, ...patch } };
      });
    },
    [expandedContactId],
  );

  // Invalidate cache when caller bumps reloadToken (trash/undo/history).
  useEffect(() => {
    if (prevReloadTokenRef.current === reloadToken) return;
    prevReloadTokenRef.current = reloadToken;
    cacheRef.current.clear();
    if (expandedContactId != null) {
      void loadContact(expandedContactId, { force: true });
    }
  }, [reloadToken, expandedContactId, loadContact]);

  // Resolve bundle when expansion or source filter changes.
  useEffect(() => {
    if (expandedContactId == null) return;
    void loadContact(expandedContactId);
  }, [expandedContactId, loadContact, sourceQuery]);

  return {
    expandedContactId,
    bundle,
    loading,
    error,
    expandContact,
    loadContact,
    invalidate,
    patchCachedDetail,
    setExpandedContactId,
  };
}

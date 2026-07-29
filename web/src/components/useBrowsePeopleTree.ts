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
  const inflightRef = useRef(
    new Map<string, Promise<ContactThreadBundle | null>>(),
  );
  const [expandedContactId, setExpandedContactId] = useState<number | null>(
    null,
  );
  const expandedContactIdRef = useRef<number | null>(null);
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
          if (expandedContactIdRef.current === contactId) {
            applyBundle(cached);
            setError(null);
            setLoading(false);
          }
          return cached;
        }
        const inflight = inflightRef.current.get(key);
        if (inflight) {
          if (expandedContactIdRef.current === contactId) {
            setLoading(true);
            setError(null);
          }
          const result = await inflight;
          if (expandedContactIdRef.current === contactId) {
            applyBundle(result);
            setLoading(false);
          }
          return result;
        }
      }

      const seq = ++seqRef.current;
      if (expandedContactIdRef.current === contactId) {
        setLoading(true);
        setError(null);
      }

      const request = (async (): Promise<ContactThreadBundle | null> => {
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
          if (!res.ok || data.error || !data.contact) {
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
          return next;
        } catch {
          return null;
        } finally {
          inflightRef.current.delete(key);
        }
      })();

      inflightRef.current.set(key, request);
      const result = await request;

      if (seq !== seqRef.current) return result;
      if (expandedContactIdRef.current !== contactId) return result;

      if (!result) {
        setError("Failed to load threads");
        applyBundle(null);
        setLoading(false);
        return null;
      }
      setError(null);
      applyBundle(result);
      setLoading(false);
      return result;
    },
    [applyBundle, sourceQuery],
  );

  const expandContact = useCallback(
    (contactId: number | null, options?: { force?: boolean }) => {
      expandedContactIdRef.current = contactId;
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
    [loadContact, applyBundle],
  );

  const invalidate = useCallback(() => {
    cacheRef.current.clear();
    inflightRef.current.clear();
    if (expandedContactIdRef.current != null) {
      void loadContact(expandedContactIdRef.current, { force: true });
    }
  }, [loadContact]);

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
        if (!prev || expandedContactIdRef.current !== contactId) return prev;
        return { ...prev, detail: { ...prev.detail, ...patch } };
      });
    },
    [],
  );

  // Invalidate cache when caller bumps reloadToken (trash/undo/history).
  useEffect(() => {
    if (prevReloadTokenRef.current === reloadToken) return;
    prevReloadTokenRef.current = reloadToken;
    cacheRef.current.clear();
    inflightRef.current.clear();
    if (expandedContactIdRef.current != null) {
      void loadContact(expandedContactIdRef.current, { force: true });
    }
  }, [reloadToken, loadContact]);

  // Source filter change: reload the expanded contact under the new key.
  // Contact expansion itself is owned by expandContact (not this effect).
  useEffect(() => {
    if (expandedContactIdRef.current == null) return;
    void loadContact(expandedContactIdRef.current);
  }, [sourceQuery, loadContact]);

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

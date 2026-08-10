import { useCallback, useEffect, useRef, useState } from "react";
import {
  PAGE_SIZE_FILL,
  PAGE_SIZE_FIRST,
  type PagedFetchPage,
} from "./usePagedList";

export type UseInfiniteOffsetListResult<T> = {
  items: T[];
  total: number;
  loading: boolean;
  refreshing: boolean;
  /** True while a scroll-triggered follow-up page is in flight. */
  filling: boolean;
  error: string;
  hasMore: boolean;
  /** Load the next page when near the end; safe to call repeatedly. */
  requestMore: () => void;
};

export type UseInfiniteOffsetListOptions = {
  firstPageSize?: number;
  fillPageSize?: number;
};

/**
 * Loads the first page for `queryKey` immediately, then loads more via
 * `requestMore` (typically when a virtual list nears the end). Aborts when
 * `queryKey` changes.
 */
export function useInfiniteOffsetList<T>(
  queryKey: string,
  fetchPage: PagedFetchPage<T>,
  options?: UseInfiniteOffsetListOptions,
): UseInfiniteOffsetListResult<T> {
  const firstPageSize = options?.firstPageSize ?? PAGE_SIZE_FIRST;
  const fillPageSize = options?.fillPageSize ?? PAGE_SIZE_FILL;

  const [items, setItems] = useState<T[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [filling, setFilling] = useState(false);
  const [error, setError] = useState("");

  const hasLoadedRef = useRef(false);
  const fetchPageRef = useRef(fetchPage);
  fetchPageRef.current = fetchPage;
  const sessionAcRef = useRef<AbortController | null>(null);
  const offsetRef = useRef(0);
  const totalRef = useRef(0);
  const loadingMoreRef = useRef(false);

  useEffect(() => {
    const ac = new AbortController();
    sessionAcRef.current = ac;
    loadingMoreRef.current = false;
    offsetRef.current = 0;
    totalRef.current = 0;

    if (hasLoadedRef.current) {
      setRefreshing(true);
      setError("");
    } else {
      setLoading(true);
      setError("");
    }
    setFilling(false);

    const run = async () => {
      try {
        const first = await fetchPageRef.current({
          limit: firstPageSize,
          offset: 0,
          signal: ac.signal,
        });
        if (ac.signal.aborted) return;

        offsetRef.current = first.items.length;
        totalRef.current = first.total;
        setItems(first.items);
        setTotal(first.total);
        hasLoadedRef.current = true;
      } catch (e) {
        if (ac.signal.aborted) return;
        setItems([]);
        setTotal(0);
        offsetRef.current = 0;
        totalRef.current = 0;
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (!ac.signal.aborted) {
          setLoading(false);
          setRefreshing(false);
          setFilling(false);
        }
      }
    };

    void run();
    return () => {
      ac.abort();
      if (sessionAcRef.current === ac) sessionAcRef.current = null;
    };
  }, [queryKey, firstPageSize]);

  const requestMore = useCallback(() => {
    const ac = sessionAcRef.current;
    if (!ac || ac.signal.aborted) return;
    if (loadingMoreRef.current) return;
    if (offsetRef.current >= totalRef.current) return;

    loadingMoreRef.current = true;
    setFilling(true);

    void (async () => {
      try {
        const page = await fetchPageRef.current({
          limit: fillPageSize,
          offset: offsetRef.current,
          signal: ac.signal,
        });
        if (ac.signal.aborted) return;

        totalRef.current = page.total;
        setTotal(page.total);
        if (page.items.length === 0) {
          offsetRef.current = totalRef.current;
          return;
        }
        offsetRef.current += page.items.length;
        setItems((prev) => [...prev, ...page.items]);
      } catch {
        /* keep what loaded; scroll can retry */
      } finally {
        loadingMoreRef.current = false;
        if (!ac.signal.aborted) setFilling(false);
      }
    })();
  }, [fillPageSize]);

  const hasMore = items.length < total;

  return {
    items,
    total,
    loading,
    refreshing,
    filling,
    error,
    hasMore,
    requestMore,
  };
}

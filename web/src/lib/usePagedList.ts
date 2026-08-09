import { useCallback, useEffect, useRef, useState } from "react";

export const PAGE_SIZE_FIRST = 40;
export const PAGE_SIZE_FILL = 100;
/** Contacts catalog first page — large enough for typical vaults in one request. */
export const PAGE_SIZE_CONTACTS_FIRST = 500;

export type PagedFetchResult<T> = {
  items: T[];
  total: number;
};

export type PagedFetchPage<T> = (args: {
  limit: number;
  offset: number;
  signal: AbortSignal;
}) => Promise<PagedFetchResult<T>>;

export type UsePagedListResult<T> = {
  items: T[];
  total: number;
  loading: boolean;
  refreshing: boolean;
  /** True while a scroll-triggered follow-up page is in flight. */
  filling: boolean;
  error: string;
  hasMore: boolean;
  loadMore: () => void;
};

export type UsePagedListOptions = {
  firstPageSize?: number;
  fillPageSize?: number;
};

/**
 * Loads the first page for `queryKey`, then loads more only via `loadMore`
 * (typically when the virtual list nears the end). Aborts when `queryKey` changes.
 */
export function usePagedList<T>(
  queryKey: string,
  fetchPage: PagedFetchPage<T>,
  options?: UsePagedListOptions,
): UsePagedListResult<T> {
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

  const loadMore = useCallback(() => {
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
    loadMore,
  };
}

/** Format a 1-based inclusive visible window against a known total. */
export function formatVisibleRange(
  visibleStart: number,
  visibleEnd: number,
  total: number,
  itemCount: number,
): string {
  if (total === 0 && itemCount === 0) return "0 of 0";
  if (itemCount === 0) return `0 of ${total}`;
  // Viewport not measured yet — show total only until VirtualList reports a window.
  if (visibleStart < 1 || visibleEnd < 1) return `… of ${total}`;
  const start = Math.min(visibleStart, itemCount);
  const end = Math.max(start, Math.min(visibleEnd, itemCount));
  return `${start}–${end} of ${total}`;
}

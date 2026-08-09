import { useCallback, useEffect, useRef, useState } from "react";

export const PAGE_SIZE_FIRST = 40;
export const PAGE_SIZE_FILL = 100;

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
  filling: boolean;
  error: string;
};

/**
 * Loads the first page immediately, then fills remaining pages in the background
 * until offset >= total. Aborts and resets when `queryKey` changes.
 */
export function usePagedList<T>(
  queryKey: string,
  fetchPage: PagedFetchPage<T>,
): UsePagedListResult<T> {
  const [items, setItems] = useState<T[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [filling, setFilling] = useState(false);
  const [error, setError] = useState("");
  const hasLoadedRef = useRef(false);
  const fetchPageRef = useRef(fetchPage);
  fetchPageRef.current = fetchPage;

  const runSession = useCallback(async (signal: AbortSignal) => {
    const fetch = fetchPageRef.current;
    if (hasLoadedRef.current) {
      setRefreshing(true);
      setError("");
    } else {
      setLoading(true);
      setError("");
    }
    setFilling(false);

    let offset = 0;
    let pageTotal = 0;

    try {
      const first = await fetch({
        limit: PAGE_SIZE_FIRST,
        offset: 0,
        signal,
      });
      if (signal.aborted) return;

      pageTotal = first.total;
      offset = first.items.length;
      setItems(first.items);
      setTotal(pageTotal);
      hasLoadedRef.current = true;
      setLoading(false);
      setRefreshing(false);

      if (offset >= pageTotal) return;

      setFilling(true);
      while (offset < pageTotal && !signal.aborted) {
        const page = await fetch({
          limit: PAGE_SIZE_FILL,
          offset,
          signal,
        });
        if (signal.aborted) return;

        pageTotal = page.total;
        if (page.items.length === 0) break;

        offset += page.items.length;
        setTotal(pageTotal);
        setItems((prev) => [...prev, ...page.items]);
      }
    } catch (e) {
      if (signal.aborted) return;
      // First-page failure clears the list; background fill failures keep what loaded.
      if (offset === 0) {
        setItems([]);
        setTotal(0);
        setError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      if (!signal.aborted) {
        setLoading(false);
        setRefreshing(false);
        setFilling(false);
      }
    }
  }, []);

  useEffect(() => {
    const ac = new AbortController();
    void runSession(ac.signal);
    return () => ac.abort();
  }, [queryKey, runSession]);

  return { items, total, loading, refreshing, filling, error };
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

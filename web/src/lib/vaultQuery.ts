/**
 * The one way the web app fetches and remembers vault data.
 *
 * TanStack Query does the remembering, the duplicate-request suppression, the
 * loading and error state, and the "this is stale now, refetch whoever is
 * showing it" broadcast. Nothing here reimplements any of that; the only thing
 * this module adds is the rule that every cache entry is named with the
 * signed-in account.
 *
 * That rule is the point. Before it, four modules kept the account's data in
 * module-level variables and `auth.tsx` cleared them by hand from two separate
 * lists — both of which named the same four and omitted the fifth, so a second
 * account could be shown the first account's Saved Searches. Naming the account
 * in the key makes that impossible rather than merely unlikely: a second
 * account asks for an entry that has never been written, finds nothing, and
 * fetches.
 *
 * See `docs/adr/0002-one-way-to-fetch-data-in-the-web-app.md`.
 */

import {
  type InfiniteData,
  QueryClient,
  type UseQueryOptions,
  type UseQueryResult,
  useInfiniteQuery,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useCallback } from "react";
import { useAuth } from "./auth";
import { PAGE_SIZE_FILL, PAGE_SIZE_FIRST } from "./listPaging";
import { ANONYMOUS_ACCOUNT, type VaultQueryKey, vaultQueryKey } from "./vaultQueryKey";

/**
 * Build the query client.
 *
 * Exported as a factory rather than a singleton so each test gets a client of
 * its own and cannot inherit another test's cache.
 */
export function createVaultQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        // The vault is usually on the same host or a local network, so a
        // refetch is cheap. Half a minute is long enough that moving between
        // screens does not refetch, and short enough that a stale list
        // corrects itself without anyone reloading.
        staleTime: 30_000,
        retry: 1,
        refetchOnWindowFocus: true,
      },
    },
  });
}

/**
 * Signed-in account id, or `"anonymous"` before sign-in.
 *
 * Queries that run on the sign-in screens have no account yet; giving them a
 * name of their own keeps their entries from ever being read by a signed-in
 * account.
 */
function useAccountScope(): string {
  const { accountId } = useAuth();
  return accountId ?? ANONYMOUS_ACCOUNT;
}

/**
 * `useQuery`, with the signed-in account added to the front of the key.
 *
 * Every option TanStack Query accepts is passed straight through. This adds no
 * caching, no fetching, and no state of its own — only the account prefix, so
 * that no call site has to remember it.
 */
export function useVaultQuery<TData>(
  key: VaultQueryKey,
  queryFn: (signal: AbortSignal) => Promise<TData>,
  options?: Omit<UseQueryOptions<TData, Error, TData>, "queryKey" | "queryFn">,
): UseQueryResult<TData, Error> {
  const account = useAccountScope();
  return useQuery<TData, Error, TData>({
    queryKey: vaultQueryKey(account, key),
    queryFn: ({ signal }) => queryFn(signal),
    ...options,
  });
}

/**
 * Mark something stale, so whatever is showing it refetches.
 *
 * This replaces the browser events the caches used to dispatch: instead of
 * naming an event and having every interested component subscribe and
 * unsubscribe, a mutation names what changed and the library refreshes whoever
 * is reading it.
 */
export function useVaultInvalidate(): (key: VaultQueryKey) => Promise<void> {
  const client = useQueryClient();
  const account = useAccountScope();
  return useCallback(
    (key: VaultQueryKey) => client.invalidateQueries({ queryKey: vaultQueryKey(account, key) }),
    [client, account],
  );
}

/**
 * Write a value straight into the cache, so a change shows without a round
 * trip.
 *
 * The vault's mutations answer with the updated value, so there is usually no
 * reason to ask for it again.
 */
export function useVaultSetCached(): <T>(key: VaultQueryKey, value: T) => void {
  const client = useQueryClient();
  const account = useAccountScope();
  return useCallback(
    <T>(key: VaultQueryKey, value: T) => {
      client.setQueryData(vaultQueryKey(account, key), value);
    },
    [client, account],
  );
}

/** One page of an offset-paged list, with the total the vault reported. */
export type OffsetPage<T> = {
  items: T[];
  total: number;
};

/** How a screen loads one page. */
export type PagedFetchPage<T> = (args: {
  limit: number;
  offset: number;
  signal: AbortSignal;
}) => Promise<OffsetPage<T>>;

/** What a long list needs to render itself while it fills. */
export type PagedListResult<T> = {
  items: T[];
  total: number;
  /** No page has arrived yet: the list has nothing to show. */
  loading: boolean;
  /** The first page is being fetched again behind rows already on screen. */
  refreshing: boolean;
  /** A later page is loading because the person scrolled near the end. */
  filling: boolean;
  error: Error | null;
  hasMore: boolean;
  loadMore: () => void;
};

/**
 * An offset-paged vault list, account-scoped like every other cache entry.
 *
 * The vault pages by `limit` and `offset` and reports a `total`, so the next
 * page starts where the pages loaded so far end and there is no next page once
 * they cover the total.
 *
 * This returns the shape a long list renders from rather than TanStack Query's
 * own result, so the two screens that use it do not each repeat the same
 * mapping from `isPending` / `isFetchingNextPage` to "loading" and "filling".
 */
export function useVaultPagedList<T>(
  key: VaultQueryKey,
  fetchPage: PagedFetchPage<T>,
  opts?: { firstPageSize?: number; fillPageSize?: number },
): PagedListResult<T> {
  const account = useAccountScope();
  const firstPageSize = opts?.firstPageSize ?? PAGE_SIZE_FIRST;
  const fillPageSize = opts?.fillPageSize ?? PAGE_SIZE_FILL;

  const query = useInfiniteQuery<
    OffsetPage<T>,
    Error,
    InfiniteData<OffsetPage<T>>,
    unknown[],
    number
  >({
    queryKey: vaultQueryKey(account, key),
    initialPageParam: 0,
    queryFn: ({ pageParam, signal }) =>
      fetchPage({
        limit: pageParam === 0 ? firstPageSize : fillPageSize,
        offset: pageParam,
        signal,
      }),
    getNextPageParam: (_lastPage, pages) => {
      const loaded = pages.reduce((sum, page) => sum + page.items.length, 0);
      const total = pages[pages.length - 1]?.total ?? 0;
      return loaded < total ? loaded : undefined;
    },
  });

  const pages = query.data?.pages ?? [];
  const items = pages.flatMap((page) => page.items);

  return {
    items,
    total: pages[pages.length - 1]?.total ?? 0,
    loading: query.isPending,
    // A refetch of what is already on screen, as opposed to a first load or a
    // page being appended.
    refreshing: query.isFetching && !query.isFetchingNextPage && !query.isPending,
    filling: query.isFetchingNextPage,
    error: query.error,
    hasMore: query.hasNextPage,
    loadMore: () => {
      if (query.hasNextPage && !query.isFetchingNextPage) void query.fetchNextPage();
    },
  };
}

export type { VaultQueryKey };

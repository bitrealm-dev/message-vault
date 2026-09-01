/** @vitest-environment jsdom */

/**
 * What this module adds on top of TanStack Query, and nothing else.
 *
 * Caching, deduplication and refetching are the library's and are not retested
 * here. What is ours is the account prefix on every key, and the mapping from
 * the library's flags to the shape a long list renders from.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { OffsetPage } from "./vaultQuery";
import { useVaultPagedList, useVaultQuery } from "./vaultQuery";

const account = { current: "account-1" };
vi.mock("./auth", () => ({
  useAuth: () => ({ accountId: account.current }),
}));

let client: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

beforeEach(() => {
  account.current = "account-1";
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
  });
});

describe("useVaultQuery", () => {
  it("names the cache entry after the signed-in account", async () => {
    const { result } = renderHook(() => useVaultQuery(["contact-groups"], async () => ["Family"]), {
      wrapper,
    });
    await waitFor(() => expect(result.current.data).toEqual(["Family"]));
    expect(client.getQueryData(["vault", "account-1", "contact-groups"])).toEqual(["Family"]);
  });

  it("does not hand one account the entry another account filled", async () => {
    const fetchGroups = vi.fn(async () => ["Family"]);
    const first = renderHook(() => useVaultQuery(["contact-groups"], fetchGroups), { wrapper });
    await waitFor(() => expect(first.result.current.data).toEqual(["Family"]));
    first.unmount();

    // A different account asks for the same thing.
    account.current = "account-2";
    fetchGroups.mockResolvedValue(["Work"]);
    const second = renderHook(() => useVaultQuery(["contact-groups"], fetchGroups), { wrapper });

    await waitFor(() => expect(second.result.current.data).toEqual(["Work"]));
    expect(fetchGroups).toHaveBeenCalledTimes(2);
  });

  it("reports the error rather than an empty result when the vault refuses", async () => {
    const { result } = renderHook(
      () =>
        useVaultQuery(["contact-groups"], async () => {
          throw new Error("nope");
        }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.error?.message).toBe("nope"));
    expect(result.current.data).toBeUndefined();
  });
});

/** A page of `count` numbered rows, out of `total`. */
function page(offset: number, count: number, total: number): OffsetPage<number> {
  return { items: Array.from({ length: count }, (_, i) => offset + i), total };
}

describe("useVaultPagedList", () => {
  it("flattens the pages loaded so far and reports the vault's total", async () => {
    const fetchPage = vi.fn(async ({ offset }: { offset: number }) => page(offset, 2, 5));
    const { result } = renderHook(
      () => useVaultPagedList(["rows"], fetchPage, { firstPageSize: 2, fillPageSize: 2 }),
      { wrapper },
    );

    await waitFor(() => expect(result.current.items).toEqual([0, 1]));
    expect(result.current.total).toBe(5);
    expect(result.current.hasMore).toBe(true);

    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.items).toEqual([0, 1, 2, 3]));
  });

  it("has no next page once the loaded rows cover the total", async () => {
    const fetchPage = vi.fn(async ({ offset }: { offset: number }) => page(offset, 2, 2));
    const { result } = renderHook(
      () => useVaultPagedList(["rows"], fetchPage, { firstPageSize: 2, fillPageSize: 2 }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.items).toEqual([0, 1]));
    expect(result.current.hasMore).toBe(false);
  });

  it("asks for the first page size first and the fill size afterwards", async () => {
    const fetchPage = vi.fn(async ({ offset }: { offset: number }) => page(offset, 3, 9));
    const { result } = renderHook(
      () => useVaultPagedList(["rows"], fetchPage, { firstPageSize: 3, fillPageSize: 7 }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.items).toHaveLength(3));
    act(() => result.current.loadMore());
    await waitFor(() => expect(fetchPage).toHaveBeenCalledTimes(2));

    expect(fetchPage.mock.calls[0]?.[0]).toMatchObject({ limit: 3, offset: 0 });
    expect(fetchPage.mock.calls[1]?.[0]).toMatchObject({ limit: 7, offset: 3 });
  });

  it("separates the first load from a later page: loading, then filling", async () => {
    let release: (() => void) | null = null;
    const fetchPage = vi.fn(async ({ offset }: { offset: number }) => {
      if (offset > 0) {
        await new Promise<void>((resolve) => {
          release = resolve;
        });
      }
      return page(offset, 2, 6);
    });

    const { result } = renderHook(
      () => useVaultPagedList(["rows"], fetchPage, { firstPageSize: 2, fillPageSize: 2 }),
      { wrapper },
    );

    expect(result.current.loading).toBe(true);
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => result.current.loadMore());
    // A later page is loading, and the rows already on screen stay put.
    await waitFor(() => expect(result.current.filling).toBe(true));
    expect(result.current.loading).toBe(false);
    expect(result.current.items).toEqual([0, 1]);

    act(() => release?.());
    await waitFor(() => expect(result.current.filling).toBe(false));
  });

  it("starts over when the key changes, rather than appending to the old list", async () => {
    const fetchPage = vi.fn(async ({ offset }: { offset: number }) => page(offset, 2, 4));
    const { result, rerender } = renderHook(
      ({ q }: { q: string }) =>
        useVaultPagedList(["rows", q], fetchPage, { firstPageSize: 2, fillPageSize: 2 }),
      { wrapper, initialProps: { q: "first" } },
    );
    await waitFor(() => expect(result.current.items).toEqual([0, 1]));
    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.items).toEqual([0, 1, 2, 3]));

    rerender({ q: "second" });
    await waitFor(() => expect(result.current.items).toEqual([0, 1]));
  });

  it("does not ask for another page while one is already loading", async () => {
    let release: (() => void) | null = null;
    const fetchPage = vi.fn(async ({ offset }: { offset: number }) => {
      if (offset > 0) {
        await new Promise<void>((resolve) => {
          release = resolve;
        });
      }
      return page(offset, 2, 10);
    });
    const { result } = renderHook(
      () => useVaultPagedList(["rows"], fetchPage, { firstPageSize: 2, fillPageSize: 2 }),
      { wrapper },
    );
    await waitFor(() => expect(result.current.items).toEqual([0, 1]));

    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.filling).toBe(true));
    act(() => result.current.loadMore());
    act(() => result.current.loadMore());

    expect(fetchPage).toHaveBeenCalledTimes(2);
    act(() => release?.());
    await waitFor(() => expect(result.current.filling).toBe(false));
  });
});

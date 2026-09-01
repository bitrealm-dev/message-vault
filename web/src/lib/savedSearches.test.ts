/** @vitest-environment jsdom */

/**
 * What this module still owns.
 *
 * The cases that used to be here for the module-level cache, the shared
 * in-flight request, and the browser event announcing a change are gone with
 * the code they covered — that is TanStack Query's job now, and
 * `vaultQuery.test.tsx` covers the part of it that is ours. What remains is
 * this module's own behaviour: the shape it reads out of a response, the ids it
 * addresses mutations by, and putting a mutation's answer where the sidebar
 * reads it.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { type SavedSearch, useSavedSearchActions, useSavedSearches } from "./savedSearches";
import {
  createSavedSearch as createVaultSavedSearch,
  deleteSavedSearch as deleteVaultSavedSearch,
  listSavedSearches,
  updateSavedSearch as updateVaultSavedSearch,
} from "./vaultApi";

const account = { current: "account-1" };
vi.mock("./auth", () => ({ useAuth: () => ({ accountId: account.current }) }));

vi.mock("./vaultApi", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./vaultApi")>()),
  listSavedSearches: vi.fn(),
  createSavedSearch: vi.fn(),
  updateSavedSearch: vi.fn(),
  deleteSavedSearch: vi.fn(),
}));

const list = vi.mocked(listSavedSearches);
const create = vi.mocked(createVaultSavedSearch);
const update = vi.mocked(updateVaultSavedSearch);
const remove = vi.mocked(deleteVaultSavedSearch);

function search(id: number, name: string, kind = "manual"): SavedSearch {
  return { id, name, query: `is:group ${name}`, kind };
}

let client: QueryClient;

function wrapper({ children }: { children: ReactNode }) {
  return createElement(QueryClientProvider, { client }, children);
}

beforeEach(() => {
  vi.clearAllMocks();
  account.current = "account-1";
  client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0, staleTime: 0 } },
  });
});

describe("useSavedSearches", () => {
  it("reads the list from the vault, not from browser storage", async () => {
    list.mockResolvedValue({ savedSearches: [search(1, "Family")] });
    const { result } = renderHook(() => useSavedSearches(), { wrapper });
    await waitFor(() => expect(result.current.savedSearches).toEqual([search(1, "Family")]));
    expect(list).toHaveBeenCalled();
  });

  it("treats a response without a list as empty rather than throwing", async () => {
    list.mockResolvedValue({});
    const { result } = renderHook(() => useSavedSearches(), { wrapper });
    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.savedSearches).toEqual([]);
  });

  /**
   * The bug this whole change exists for.
   *
   * Saved searches used to be held in a module-level variable that `auth.tsx`
   * had to clear by hand on sign-in and sign-out. Both of its clearing lists
   * named four other caches and omitted this one, so signing in as a second
   * account showed the first account's saved searches until something else
   * refreshed them. The entry is now named with the account, so the second
   * account asks for something that was never written.
   */
  it("does not show one account the saved searches of another", async () => {
    // Entries are kept and treated as fresh here, so a key that did not name
    // the account would serve the first account's list to the second.
    client = new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
          gcTime: Number.POSITIVE_INFINITY,
          staleTime: Number.POSITIVE_INFINITY,
        },
      },
    });

    list.mockResolvedValue({ savedSearches: [search(1, "Alice's Family")] });
    const first = renderHook(() => useSavedSearches(), { wrapper });
    await waitFor(() =>
      expect(first.result.current.savedSearches).toEqual([search(1, "Alice's Family")]),
    );
    first.unmount();

    account.current = "account-2";
    list.mockResolvedValue({ savedSearches: [search(2, "Bob's Work")] });
    const second = renderHook(() => useSavedSearches(), { wrapper });

    await waitFor(() =>
      expect(second.result.current.savedSearches).toEqual([search(2, "Bob's Work")]),
    );
    expect(second.result.current.savedSearches).not.toContainEqual(search(1, "Alice's Family"));
  });

  it("keeps the kind the vault reports, so import rows stay identifiable", async () => {
    list.mockResolvedValue({ savedSearches: [search(2, "Backup 1", "import")] });
    const { result } = renderHook(() => useSavedSearches(), { wrapper });
    await waitFor(() => expect(result.current.savedSearches[0]?.kind).toBe("import"));
  });
});

describe("useSavedSearchActions", () => {
  it("addresses an update by id, sending both fields", async () => {
    update.mockResolvedValue({ savedSearches: [search(3, "Renamed")] });
    const { result } = renderHook(() => useSavedSearchActions(), { wrapper });
    await result.current.update(3, "Renamed", "is:direct");
    expect(update).toHaveBeenCalledWith(3, { name: "Renamed", query: "is:direct" });
  });

  it("addresses a delete by id", async () => {
    remove.mockResolvedValue({ savedSearches: [] });
    const { result } = renderHook(() => useSavedSearchActions(), { wrapper });
    await result.current.remove(7);
    expect(remove).toHaveBeenCalledWith(7);
  });

  it("takes the refreshed list from a mutation instead of asking again", async () => {
    list.mockResolvedValue({ savedSearches: [] });
    const both = renderHook(
      () => ({ read: useSavedSearches(), actions: useSavedSearchActions() }),
      { wrapper },
    );
    await waitFor(() => expect(both.result.current.read.loading).toBe(false));
    expect(list).toHaveBeenCalledTimes(1);

    create.mockResolvedValue({ savedSearches: [search(1, "Family")] });
    await both.result.current.actions.create("Family", "is:group");

    // The list a mutation answered with is what the sidebar shows, with no
    // second request.
    await waitFor(() =>
      expect(both.result.current.read.savedSearches).toEqual([search(1, "Family")]),
    );
    expect(list).toHaveBeenCalledTimes(1);
  });
});

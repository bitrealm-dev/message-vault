import { type UseMutationResult, useMutation } from "@tanstack/react-query";
import { useMemo } from "react";
import {
  createSavedSearch as createVaultSavedSearch,
  deleteSavedSearch as deleteVaultSavedSearch,
  listSavedSearches,
  updateSavedSearch as updateVaultSavedSearch,
} from "./vaultApi";
import type { components } from "./vaultApi.types";
import { keys } from "./vaultKeys";
import { useVaultCache, useVaultQuery } from "./vaultQuery";

type Schema = components["schemas"];

/**
 * Saved searches live in the vault, not in the browser. They belong to an
 * account, so they follow a person to another machine and go away when the
 * vault's data does.
 *
 * Unlike Contact Groups and Message Tags this is not a `nameCollection`: a
 * saved search carries a name *and* a query, so it is addressed by id and
 * cannot use that factory's names-only shape.
 *
 * The module-level cache and the browser event that used to live here are gone.
 * They are the reason a second account could be shown the first account's saved
 * searches: `auth.tsx` cleared four other caches by hand and missed this one.
 * The cache entry is now named with the account, so there is nothing to
 * remember.
 */

export type SavedSearch = Schema["SavedSearch"];

/** The account's saved searches, A–Z as the vault orders them. */
export function useSavedSearches(): {
  savedSearches: SavedSearch[];
  loading: boolean;
} {
  const { data, isPending } = useVaultQuery(
    keys.savedSearches.all,
    async (signal) => (await listSavedSearches({ signal })).items,
  );
  return { savedSearches: data ?? [], loading: isPending };
}

/** Every write is followed by one fresh read of the list the sidebar shows. */
function useSavedSearchWrite<V>(
  write: (vars: V) => Promise<unknown>,
): UseMutationResult<unknown, Error, V> {
  const cache = useVaultCache();
  return useMutation<unknown, Error, V>({
    mutationFn: write,
    onSettled: () => cache.invalidate(keys.savedSearches.all),
  });
}

export function useCreateSavedSearch(): UseMutationResult<
  unknown,
  Error,
  { name: string; query: string }
> {
  return useSavedSearchWrite((body) => createVaultSavedSearch(body));
}

export function useUpdateSavedSearch(): UseMutationResult<
  unknown,
  Error,
  { id: number; name: string; query: string }
> {
  return useSavedSearchWrite(({ id, name, query }) => updateVaultSavedSearch(id, { name, query }));
}

export function useDeleteSavedSearch(): UseMutationResult<unknown, Error, number> {
  return useSavedSearchWrite((id) => deleteVaultSavedSearch(id));
}

export type SavedSearchActions = {
  create: (name: string, query: string) => Promise<void>;
  update: (id: number, name: string, query: string) => Promise<void>;
  remove: (id: number) => Promise<void>;
  /** Any of the three in flight. */
  pending: boolean;
  /** The most recent failure, or null. */
  error: Error | null;
};

/** Create, rename and delete saved searches, as the sidebar asks for them. */
export function useSavedSearchActions(): SavedSearchActions {
  const createSearch = useCreateSavedSearch();
  const updateSearch = useUpdateSavedSearch();
  const deleteSearch = useDeleteSavedSearch();

  const create = createSearch.mutateAsync;
  const update = updateSearch.mutateAsync;
  const remove = deleteSearch.mutateAsync;
  const pending = createSearch.isPending || updateSearch.isPending || deleteSearch.isPending;

  // Each mutate call resets that mutation's own error and stamps a fresh
  // `submittedAt`, so whichever of the three last started is also whichever
  // last settled; its error (or lack of one) is the actions' error. A fixed
  // create-then-update-then-remove order would instead let an old create
  // failure outlive every write that came after it.
  const latest = [createSearch, updateSearch, deleteSearch].reduce((newest, next) =>
    next.submittedAt > newest.submittedAt ? next : newest,
  );
  const error = latest.error;

  // Memoised on the mutations' own stable `mutateAsync` identities only:
  // `pending` and `error` change on every keystroke of a write, and a caller
  // that lists these methods in a `useEffect` dependency array must not see a
  // new function each time.
  const callbacks = useMemo(
    () => ({
      create: async (name: string, query: string) => {
        await create({ name, query });
      },
      update: async (id: number, name: string, query: string) => {
        await update({ id, name, query });
      },
      remove: async (id: number) => {
        await remove(id);
      },
    }),
    [create, update, remove],
  );

  return { ...callbacks, pending, error };
}

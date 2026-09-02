import { useMemo } from "react";
import {
  createSavedSearch as createVaultSavedSearch,
  deleteSavedSearch as deleteVaultSavedSearch,
  listSavedSearches,
  updateSavedSearch as updateVaultSavedSearch,
} from "./vaultApi";
import { useVaultQuery, useVaultSetCached } from "./vaultQuery";
import { keys } from "./vaultKeys";

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

export interface SavedSearch {
  id: number;
  name: string;
  query: string;
  /** `manual` when a person wrote it, `import` when an import run created it. */
  kind: string;
}

type ListResponse = { savedSearches?: SavedSearch[] };

function listFrom(res: ListResponse): SavedSearch[] {
  return Array.isArray(res.savedSearches) ? res.savedSearches : [];
}

/** The account's saved searches, A–Z as the vault orders them. */
export function useSavedSearches(): {
  savedSearches: SavedSearch[];
  loading: boolean;
} {
  const { data, isPending } = useVaultQuery(keys.savedSearches.all, async (signal) =>
    listFrom(await listSavedSearches({ signal })),
  );
  return { savedSearches: data ?? [], loading: isPending };
}

/**
 * Create, rename, and delete saved searches.
 *
 * Every mutation answers with the refreshed list, so each writes that straight
 * into the cache and the sidebar updates without a second round trip.
 */
export function useSavedSearchActions(): {
  create: (name: string, query: string) => Promise<void>;
  update: (id: number, name: string, query: string) => Promise<void>;
  remove: (id: number) => Promise<void>;
} {
  const setCached = useVaultSetCached();
  // One stable object, for the same reason as the name collections.
  return useMemo(() => {
    const adopt = (res: ListResponse) => {
      setCached(keys.savedSearches.all, listFrom(res));
    };
    return {
      async create(name: string, query: string) {
        adopt(await createVaultSavedSearch({ name, query }));
      },
      async update(id: number, name: string, query: string) {
        adopt(await updateVaultSavedSearch(id, { name, query }));
      },
      async remove(id: number) {
        adopt(await deleteVaultSavedSearch(id));
      },
    };
  }, [setCached]);
}

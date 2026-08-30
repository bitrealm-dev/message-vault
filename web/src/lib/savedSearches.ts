import { useCallback, useEffect, useState } from "react";
import { apiClient } from "./api";
import { useAuth } from "./auth";

/**
 * Saved searches live in the vault, not in the browser. They belong to an
 * account, so they follow a person to another machine and go away when the
 * vault's data does.
 *
 * Unlike contact groups and message tags this is not a `nameCollection`: a
 * saved search carries a name *and* a query, so it is addressed by id and
 * cannot use that factory's names-only shape.
 */

export interface SavedSearch {
  id: number;
  name: string;
  query: string;
  /** `manual` when a person wrote it, `import` when an import run created it. */
  kind: string;
}

export const SAVED_SEARCHES_CHANGED_EVENT = "mv-saved-searches-changed";

type ListResponse = { savedSearches?: SavedSearch[] };

let cached: SavedSearch[] | null = null;
let inflight: Promise<SavedSearch[]> | null = null;

function listFrom(res: ListResponse): SavedSearch[] {
  return Array.isArray(res.savedSearches) ? res.savedSearches : [];
}

/**
 * Tell the open UI the list changed.
 *
 * This deliberately leaves the cache alone: every mutation returns the
 * refreshed list, so listeners can read it without a second round trip.
 */
function notifyChanged(): void {
  try {
    globalThis.dispatchEvent?.(new Event(SAVED_SEARCHES_CHANGED_EVENT));
  } catch {
    // Some browsers block custom events. The next fetch still works.
  }
}

/** Record the list a mutation returned, announce it, and hand it back. */
function adopt(res: ListResponse): SavedSearch[] {
  const list = listFrom(res);
  cached = list;
  notifyChanged();
  return list;
}

/** The account's saved searches, A–Z as the server orders them. */
export async function fetchSavedSearches(signal?: AbortSignal): Promise<SavedSearch[]> {
  if (cached !== null && !signal) return cached;
  if (inflight && !signal) return inflight;
  const req = apiClient
    .get<ListResponse>("/v1/saved-searches", { signal })
    .then((res) => {
      const list = listFrom(res);
      cached = list;
      return list;
    })
    .finally(() => {
      inflight = null;
    });
  if (!signal) inflight = req;
  return req;
}

export async function createSavedSearch(name: string, query: string): Promise<SavedSearch[]> {
  return adopt(await apiClient.post<ListResponse>("/v1/saved-searches", { name, query }));
}

/** Replace a saved search's name and query. The id and kind do not change. */
export async function updateSavedSearch(
  id: number,
  name: string,
  query: string,
): Promise<SavedSearch[]> {
  return adopt(await apiClient.patch<ListResponse>(`/v1/saved-searches/${id}`, { name, query }));
}

/**
 * Delete a saved search.
 *
 * An import's saved search is only a shortcut to that run's messages. Removing
 * it leaves the import itself recorded in the vault, where Import History
 * still shows it.
 */
export async function deleteSavedSearch(id: number): Promise<SavedSearch[]> {
  return adopt(await apiClient.delete<ListResponse>(`/v1/saved-searches/${id}`));
}

/** Drop the module cache, so the next read goes to the vault. */
export function invalidateSavedSearches(): void {
  cached = null;
  notifyChanged();
}

/** Live list of the signed-in account's saved searches. */
export function useSavedSearches(): {
  savedSearches: SavedSearch[];
  loading: boolean;
  refresh: () => Promise<void>;
} {
  const { isAuthenticated, token } = useAuth();
  const [savedSearches, setSavedSearches] = useState<SavedSearch[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      setSavedSearches(await fetchSavedSearches());
    } catch {
      /* Keep the last good list. A failed refresh must not hide existing rows. */
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!isAuthenticated || !token) return;
    void refresh();
    const onChange = () => {
      void refresh();
    };
    globalThis.addEventListener(SAVED_SEARCHES_CHANGED_EVENT, onChange);
    return () => {
      globalThis.removeEventListener(SAVED_SEARCHES_CHANGED_EVENT, onChange);
    };
  }, [isAuthenticated, refresh, token]);

  return { savedSearches, loading, refresh };
}

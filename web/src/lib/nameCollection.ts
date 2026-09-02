import { useMemo } from "react";
import {
  useVaultCached,
  useVaultFetchFresh,
  useVaultInvalidate,
  useVaultQuery,
  type VaultQueryKey,
} from "./vaultQuery";

/**
 * Contact Groups and Message Tags are the same feature over different nouns: a
 * named set the account owns, and a membership that puts rows in or out of it.
 * This builds one from a description of the nouns so the two do not drift
 * apart.
 *
 * The vault addresses a set by its id; screens, the sidebar, and the router
 * hold names. The lookup from one to the other lives here and nowhere else:
 * the id comes from the cached list, or from the vault once when the cached
 * list does not hold the name, and a name the vault does not know is an error
 * before any request is sent. See `docs/adr/0003`.
 */

/** One set as the vault answers it. */
export type NamedSet = { id: number; name: string };

/** Members to put in and take out of a set, in one request. Either side may be left off. */
export type MembersPatch = { add?: number[]; remove?: number[] };

/** The vault calls one of these collections is built from. */
export type NameCollectionRoutes = {
  list: (opts?: { signal?: AbortSignal }) => Promise<{ items: NamedSet[] }>;
  create: (body: { name: string }) => Promise<NamedSet>;
  update: (id: number, body: { name: string }) => Promise<NamedSet>;
  remove: (id: number) => Promise<void>;
  updateMembers: (
    id: number,
    body: { add: number[]; remove: number[] },
  ) => Promise<{ added: number; removed: number }>;
};

export type NameCollectionConfig = {
  routes: NameCollectionRoutes;
  /** This collection's cache prefix, from `vaultKeys`. */
  key: VaultQueryKey;
  /**
   * Cache keys of the lists that show these names as chips, invalidated after
   * every write. Matched by prefix, so `keys.contacts.all` covers every page
   * and every search of the contact list.
   */
  invalidates: readonly VaultQueryKey[];
  /** What one of these is called in an error, e.g. `group`. */
  label: string;
  /** Search token used in list queries, e.g. `group` for `group:Family`. */
  queryToken: string;
  reservedNames: ReadonlySet<string>;
  reservedError: (name: string) => string;
};

export type NameCollection = {
  /** Cache key parts, before the account is put in front of them. */
  key: VaultQueryKey;
  routes: NameCollectionRoutes;
  invalidates: readonly VaultQueryKey[];
  label: string;
  isReserved: (name: string) => boolean;
  reservedError: (name: string) => string;
  /** Build the list query for one page of this collection plus a typed search. */
  listQuery: (name: string | "none" | null, search: string) => string;
};

export function createNameCollection(config: NameCollectionConfig): NameCollection {
  const isReserved = (name: string) => config.reservedNames.has(name.trim().toLowerCase());

  function listQuery(name: string | "none" | null, search: string): string {
    const parts: string[] = [];
    if (name === "none") {
      parts.push(`${config.queryToken}:none`);
    } else if (name) {
      parts.push(
        /\s/.test(name) ? `${config.queryToken}:"${name}"` : `${config.queryToken}:${name}`,
      );
    }
    const extra = search.trim();
    if (extra) parts.push(extra);
    return parts.join(" ");
  }

  return {
    key: config.key,
    routes: config.routes,
    invalidates: config.invalidates,
    label: config.label,
    isReserved,
    reservedError: config.reservedError,
    listQuery,
  };
}

/** The cache holds the vault's list as it came, ids included. */
async function fetchSets(collection: NameCollection, signal: AbortSignal): Promise<NamedSet[]> {
  return (await collection.routes.list({ signal })).items;
}

/** Live list of one collection's names for the signed-in account. */
export function useNameCollection(collection: NameCollection): {
  names: string[];
  loading: boolean;
} {
  const { data, isPending } = useVaultQuery(collection.key, (signal) =>
    fetchSets(collection, signal),
  );
  const names = useMemo(() => (data ?? []).map((set) => set.name), [data]);
  return { names, loading: isPending };
}

/**
 * The four things a person can do to one of these collections.
 *
 * Every write invalidates the collection's own list and the lists that show
 * its names as chips, so a renamed or deleted group disappears from contact
 * rows without anyone reloading.
 */
export function useNameCollectionActions(collection: NameCollection): {
  create: (name: string) => Promise<string>;
  rename: (from: string, to: string) => Promise<string>;
  remove: (name: string) => Promise<void>;
  setMembers: (name: string, patch: MembersPatch) => Promise<{ added: number; removed: number }>;
  invalidate: () => Promise<void>;
} {
  const cached = useVaultCached();
  const fetchFresh = useVaultFetchFresh();
  const invalidate = useVaultInvalidate();

  // One stable object, so a caller can list it as a dependency without
  // rebuilding every callback that uses it on each render.
  return useMemo(() => {
    const findId = (sets: NamedSet[] | undefined, name: string): number | undefined => {
      const wanted = name.trim().toLowerCase();
      return sets?.find((set) => set.name.toLowerCase() === wanted)?.id;
    };

    /**
     * The id behind a name: from the cache, else from the vault once, else an
     * error and no request. The vault-once path covers creating a set and
     * adding to it before the invalidated list has come back.
     */
    async function idOf(name: string): Promise<number> {
      const hit = findId(cached<NamedSet[]>(collection.key), name);
      if (hit !== undefined) return hit;
      const fresh = findId(
        await fetchFresh(collection.key, (signal) => fetchSets(collection, signal)),
        name,
      );
      if (fresh !== undefined) return fresh;
      throw new Error(`${collection.label} not found`);
    }

    const staleKeys: readonly VaultQueryKey[] = [collection.key, ...collection.invalidates];
    const changed = async () => {
      await Promise.all(staleKeys.map((key) => invalidate(key)));
    };

    const checkName = (name: string): string => {
      const trimmed = name.trim();
      if (!trimmed) throw new Error("name required");
      if (collection.isReserved(trimmed)) throw new Error(collection.reservedError(trimmed));
      return trimmed;
    };

    return {
      async create(name: string) {
        const trimmed = checkName(name);
        const created = await collection.routes.create({ name: trimmed });
        await changed();
        return created.name;
      },
      async rename(from: string, to: string) {
        const trimmed = checkName(to);
        const id = await idOf(from);
        const updated = await collection.routes.update(id, { name: trimmed });
        await changed();
        return updated.name;
      },
      async remove(name: string) {
        const id = await idOf(name);
        await collection.routes.remove(id);
        await changed();
      },
      async setMembers(name: string, patch: MembersPatch) {
        const id = await idOf(name);
        const result = await collection.routes.updateMembers(id, {
          add: patch.add ?? [],
          remove: patch.remove ?? [],
        });
        await changed();
        return result;
      },
      invalidate: () => invalidate(collection.key),
    };
  }, [collection, cached, fetchFresh, invalidate]);
}

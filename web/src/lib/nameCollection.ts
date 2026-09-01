import { useMemo } from "react";
import { useVaultInvalidate, useVaultQuery, useVaultSetCached } from "./vaultQuery";

/**
 * Contact Groups and Message Tags are the same feature over different nouns: a
 * named set the account owns, and a membership call that puts rows in or out of
 * it. This builds one from a description of the nouns so the two do not drift
 * apart.
 *
 * What used to live here as well — a module-level cache, an in-flight guard,
 * and a browser event telling the open interface to refetch — is TanStack
 * Query's now. A mutation says what changed and whoever is showing it
 * refreshes; nothing subscribes to an event, and nothing has to be cleared when
 * the account changes.
 */

/** The four vault calls one of these collections is built from. */
export type NameCollectionRoutes = {
  list: (opts?: { signal?: AbortSignal }) => Promise<Record<string, unknown>>;
  create: (body: { name: string }) => Promise<Record<string, unknown>>;
  rename: (body: { from: string; to: string }) => Promise<Record<string, unknown>>;
  remove: (body: { name: string }) => Promise<Record<string, unknown>>;
  setMembership: (body: {
    ids: number[];
    name: string;
    enable: boolean;
  }) => Promise<{ changed: number }>;
};

export type NameCollectionConfig = {
  /** The vault calls this collection is made of. */
  routes: NameCollectionRoutes;
  /** Name of this collection in a cache key, e.g. `contact-groups`. */
  cacheKey: string;
  /** Key holding the name array in every list or mutation response. */
  responseKey: string;
  /** Search token used in list queries, e.g. `group` for `group:Family`. */
  queryToken: string;
  reservedNames: ReadonlySet<string>;
  reservedError: (name: string) => string;
};

export type NameCollection = {
  /** Cache key parts, before the account is put in front of them. */
  key: readonly [string];
  routes: NameCollectionRoutes;
  responseKey: string;
  isReserved: (name: string) => boolean;
  reservedError: (name: string) => string;
  namesFrom: (res: Record<string, unknown>) => string[];
  /** Build the list query for one page of this collection plus a typed search. */
  listQuery: (name: string | "none" | null, search: string) => string;
};

export function createNameCollection(config: NameCollectionConfig): NameCollection {
  const namesFrom = (res: Record<string, unknown>): string[] => {
    const value = res[config.responseKey];
    return Array.isArray(value) ? (value as string[]) : [];
  };

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
    key: [config.cacheKey] as const,
    routes: config.routes,
    responseKey: config.responseKey,
    isReserved,
    reservedError: config.reservedError,
    namesFrom,
    listQuery,
  };
}

/** Live list of one collection's names for the signed-in account. */
export function useNameCollection(collection: NameCollection): {
  names: string[];
  loading: boolean;
} {
  const { data, isPending } = useVaultQuery(collection.key, async (signal) =>
    collection.namesFrom(await collection.routes.list({ signal })),
  );
  return { names: data ?? [], loading: isPending };
}

/**
 * The four things a person can do to one of these collections.
 *
 * Each mutation writes the list the vault answered with straight into the
 * cache, so the sidebar updates without a second round trip, and membership
 * changes invalidate instead, since they change rows rather than names.
 */
export function useNameCollectionActions(collection: NameCollection): {
  create: (name: string) => Promise<string>;
  rename: (from: string, to: string) => Promise<string>;
  remove: (name: string) => Promise<void>;
  setMembership: (ids: number[], name: string, enable: boolean) => Promise<number>;
  invalidate: () => Promise<void>;
} {
  const setCached = useVaultSetCached();
  const invalidate = useVaultInvalidate();

  // One stable object, so a caller can list it as a dependency without
  // rebuilding every callback that uses it on each render.
  return useMemo(
    () => ({
      async create(name: string) {
        const trimmed = name.trim();
        if (!trimmed) throw new Error("name required");
        if (collection.isReserved(trimmed)) throw new Error(collection.reservedError(trimmed));
        const res = await collection.routes.create({ name: trimmed });
        setCached(collection.key, collection.namesFrom(res));
        return String(res.name);
      },
      async rename(from: string, to: string) {
        const res = await collection.routes.rename({ from, to });
        setCached(collection.key, collection.namesFrom(res));
        return String(res.name);
      },
      async remove(name: string) {
        const res = await collection.routes.remove({ name });
        setCached(collection.key, collection.namesFrom(res));
      },
      async setMembership(ids: number[], name: string, enable: boolean) {
        const res = await collection.routes.setMembership({ ids, name, enable });
        await invalidate(collection.key);
        return res.changed;
      },
      invalidate: () => invalidate(collection.key),
    }),
    [collection, setCached, invalidate],
  );
}

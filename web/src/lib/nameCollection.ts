import { useCallback, useEffect, useState } from "react";
import { useAuth } from "./auth";

/**
 * Contact groups and message tags are the same feature over different nouns: a
 * named set the account owns, a membership call that puts rows in or out of it,
 * a module-level cache with an in-flight guard, and a DOM event that tells the
 * open UI to refetch. This builds one from a description of the nouns so the
 * two do not drift apart — they had already started to, with `messageTags`
 * re-exporting `contactGroups`' slug helpers verbatim.
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
  /** Key holding the name array in every list or mutation response. */
  responseKey: string;
  /** Search token used in list queries, e.g. `group` for `group:Family`. */
  queryToken: string;
  /** DOM event fired when the collection changes. */
  changedEvent: string;
  reservedNames: ReadonlySet<string>;
  reservedError: (name: string) => string;
};

export type NameCollection = {
  changedEvent: string;
  isReserved: (name: string) => boolean;
  reservedError: (name: string) => string;
  fetchAll: (signal?: AbortSignal) => Promise<string[]>;
  invalidate: () => void;
  create: (name: string) => Promise<string>;
  rename: (from: string, to: string) => Promise<string>;
  remove: (name: string) => Promise<void>;
  setMembership: (ids: number[], name: string, enable: boolean) => Promise<number>;
  /** Build the list query for one page of this collection plus a typed search. */
  listQuery: (name: string | "none" | null, search: string) => string;
};

export function createNameCollection(config: NameCollectionConfig): NameCollection {
  let cached: string[] | null = null;
  let inflight: Promise<string[]> | null = null;

  const namesFrom = (res: Record<string, unknown>): string[] => {
    const value = res[config.responseKey];
    return Array.isArray(value) ? (value as string[]) : [];
  };

  function notifyChanged(): void {
    cached = null;
    try {
      globalThis.dispatchEvent?.(new Event(config.changedEvent));
    } catch {
      // Some browsers block custom events. The next fetch still works.
    }
  }

  const isReserved = (name: string) => config.reservedNames.has(name.trim().toLowerCase());

  async function fetchAll(signal?: AbortSignal): Promise<string[]> {
    if (cached !== null && !signal) return cached;
    if (inflight && !signal) return inflight;
    const req = config.routes
      .list({ signal })
      .then((res) => {
        const names = namesFrom(res);
        cached = names;
        return names;
      })
      .finally(() => {
        inflight = null;
      });
    if (!signal) inflight = req;
    return req;
  }

  async function create(name: string): Promise<string> {
    const trimmed = name.trim();
    if (!trimmed) throw new Error("name required");
    if (isReserved(trimmed)) throw new Error(config.reservedError(trimmed));
    const res = await config.routes.create({ name: trimmed });
    cached = namesFrom(res);
    notifyChanged();
    return String(res.name);
  }

  async function rename(from: string, to: string): Promise<string> {
    const res = await config.routes.rename({ from, to });
    cached = namesFrom(res);
    notifyChanged();
    return String(res.name);
  }

  async function remove(name: string): Promise<void> {
    const res = await config.routes.remove({ name });
    cached = namesFrom(res);
    notifyChanged();
  }

  async function setMembership(ids: number[], name: string, enable: boolean): Promise<number> {
    const res = await config.routes.setMembership({ ids, name, enable });
    notifyChanged();
    return res.changed;
  }

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
    changedEvent: config.changedEvent,
    isReserved,
    reservedError: config.reservedError,
    fetchAll,
    invalidate: notifyChanged,
    create,
    rename,
    remove,
    setMembership,
    listQuery,
  };
}

/** Live list of one collection's names for the signed-in account. */
export function useNameCollection(collection: NameCollection): {
  names: string[];
  loading: boolean;
  refresh: () => Promise<void>;
} {
  const { isAuthenticated, token } = useAuth();
  const [names, setNames] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      setNames(await collection.fetchAll());
    } catch {
      /* Keep the last good list. A failed refresh must not hide existing names. */
    } finally {
      setLoading(false);
    }
  }, [collection]);

  useEffect(() => {
    if (!isAuthenticated || !token) return;
    void refresh();
    const onChange = () => {
      void refresh();
    };
    globalThis.addEventListener(collection.changedEvent, onChange);
    return () => {
      globalThis.removeEventListener(collection.changedEvent, onChange);
    };
  }, [isAuthenticated, refresh, token, collection.changedEvent]);

  return { names, loading, refresh };
}

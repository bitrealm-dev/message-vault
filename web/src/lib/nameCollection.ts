import { useCallback, useEffect, useState } from "react";
import { apiClient } from "./api";
import { useAuth } from "./auth";

/**
 * Contact groups and thread tags are the same feature over different nouns: a
 * named set the account owns, a membership call that puts rows in or out of it,
 * a module-level cache with an in-flight guard, and a DOM event that tells the
 * open UI to refetch. This builds one from a description of the nouns so the
 * two do not drift apart — they had already started to, with `threadTags`
 * re-exporting `contactGroups`' slug helpers verbatim.
 */

export type NameCollectionConfig = {
  /** Collection endpoint, e.g. `/v1/contact-groups`. */
  endpoint: string;
  /** Membership endpoint, e.g. `/v1/contacts/groups`. */
  membershipEndpoint: string;
  /** Key holding the name array in every response from `endpoint`. */
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
    const req = apiClient
      .get<Record<string, unknown>>(config.endpoint, { signal })
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
    const res = await apiClient.post<Record<string, unknown>>(config.endpoint, { name: trimmed });
    cached = namesFrom(res);
    notifyChanged();
    return String(res.name);
  }

  async function rename(from: string, to: string): Promise<string> {
    const res = await apiClient.patch<Record<string, unknown>>(config.endpoint, { from, to });
    cached = namesFrom(res);
    notifyChanged();
    return String(res.name);
  }

  async function remove(name: string): Promise<void> {
    const res = await apiClient.delete<Record<string, unknown>>(config.endpoint, { name });
    cached = namesFrom(res);
    notifyChanged();
  }

  async function setMembership(ids: number[], name: string, enable: boolean): Promise<number> {
    const res = await apiClient.post<{ changed: number }>(config.membershipEndpoint, {
      ids,
      name,
      enable,
    });
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

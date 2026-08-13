/** In-memory cache of contact details so reopening a drawer is instant. */

export type CachedContactHandle = {
  handle: string;
  service: string | null;
  name_alias?: string | null;
  start_date: string | null;
  end_date: string | null;
  individual_conversations: number;
  group_conversations: number;
  individual_message_count: number;
  group_message_count: number;
};

export type CachedContactDetail = {
  id: string;
  name: string;
  handles: CachedContactHandle[];
  direct_conversations: number;
  group_conversations: number;
  total_messages: number;
  last_modified?: string;
};

const cache = new Map<string, CachedContactDetail>();
const inflight = new Map<string, Promise<CachedContactDetail>>();

/** Return a cached contact detail, or null when this id has not been loaded. */
export function getCachedContactDetail(id: string): CachedContactDetail | null {
  return cache.get(String(id)) ?? null;
}

function setCachedContactDetail(detail: CachedContactDetail): void {
  cache.set(String(detail.id), detail);
}

/** Drop one contact from the cache so the next open loads a fresh copy. */
export function invalidateContactDetail(id: string): void {
  const key = String(id);
  cache.delete(key);
  inflight.delete(key);
}

/** Drop every cached contact. Used on login and logout. */
export function clearContactDetailCache(): void {
  cache.clear();
  inflight.clear();
}

/**
 * Load one contact's details. Returns a cached copy when present.
 * If the same id is requested twice at once, only one network request runs.
 */
export async function fetchContactDetail(
  id: string,
  get: (path: string, opts?: { signal?: AbortSignal }) => Promise<CachedContactDetail>,
  signal?: AbortSignal,
): Promise<CachedContactDetail> {
  const key = String(id);
  const existing = inflight.get(key);
  if (existing) {
    return existing;
  }

  const promise = get(`/v1/export/contacts/${key}`, { signal })
    .then((detail) => {
      setCachedContactDetail(detail);
      return detail;
    })
    .finally(() => {
      inflight.delete(key);
    });

  inflight.set(key, promise);
  return promise;
}

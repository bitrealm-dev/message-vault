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
  groups?: string[];
};

const cache = new Map<string, CachedContactDetail>();
const inflight = new Map<string, Promise<CachedContactDetail>>();

/** Fired when a cached contact's groups (or other fields) change in place. */
export const CONTACT_DETAIL_CHANGED_EVENT = "mv-contact-detail-changed";

function notifyContactDetailChanged(id: string, groups: string[]): void {
  try {
    globalThis.dispatchEvent?.(
      new CustomEvent(CONTACT_DETAIL_CHANGED_EVENT, {
        detail: { id: String(id), groups },
      }),
    );
  } catch {
    /* private mode */
  }
}

/** Return a cached contact detail, or null when this id has not been loaded. */
export function getCachedContactDetail(id: string): CachedContactDetail | null {
  return cache.get(String(id)) ?? null;
}

function setCachedContactDetail(detail: CachedContactDetail): void {
  cache.set(String(detail.id), detail);
}

/** Write group names onto a cached contact and tell open drawers to refresh. */
export function updateCachedContactGroups(id: string, groups: string[]): void {
  const key = String(id);
  const current = cache.get(key);
  if (current) {
    cache.set(key, { ...current, groups });
  }
  notifyContactDetailChanged(key, groups);
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

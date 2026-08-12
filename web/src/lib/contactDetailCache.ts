/** In-memory contact detail cache (Next-like instant reopen). */

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

export function getCachedContactDetail(id: string): CachedContactDetail | null {
  return cache.get(String(id)) ?? null;
}

function setCachedContactDetail(detail: CachedContactDetail): void {
  cache.set(String(detail.id), detail);
}

export function invalidateContactDetail(id: string): void {
  const key = String(id);
  cache.delete(key);
  inflight.delete(key);
}

export function clearContactDetailCache(): void {
  cache.clear();
  inflight.clear();
}

/**
 * Cache-first fetch: returns cached detail immediately via callback path in the
 * drawer; this helper dedupes in-flight GETs and stores the result.
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

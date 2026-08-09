/** Browser-side sync for account display prefs (localStorage cache + server). */

let fetchPromise: Promise<Record<string, string> | null> | null = null;

/** One shared GET per page load; null on 401 / network error. */
export function fetchServerPrefs(): Promise<Record<string, string> | null> {
  if (typeof window === "undefined") return Promise.resolve(null);
  if (!fetchPromise) {
    fetchPromise = (async () => {
      try {
        const res = await fetch("/api/settings/prefs", {
          credentials: "same-origin",
        });
        if (!res.ok) return null;
        const data = (await res.json()) as { prefs?: Record<string, string> };
        return data.prefs && typeof data.prefs === "object" ? data.prefs : {};
      } catch {
        return null;
      }
    })();
  }
  return fetchPromise;
}

/** Fire-and-forget PATCH; errors are swallowed. */
export function pushServerPrefs(patch: Record<string, string>): void {
  if (typeof window === "undefined") return;
  if (Object.keys(patch).length === 0) return;
  void fetch("/api/settings/prefs", {
    method: "PATCH",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ prefs: patch }),
  }).catch(() => {
    /* ignore */
  });
}

export type ReconcileResult = {
  /** Effective values after reconcile (server wins when present). */
  values: Record<string, string | null>;
  /** Local keys to push because the server had no value yet. */
  toPush: Record<string, string>;
};

/**
 * For each key: server value wins (written to localStorage).
 * If server lacks a key but localStorage has one, queue it for a one-time push.
 */
export function reconcilePrefs(
  serverPrefs: Record<string, string>,
  keys: readonly string[],
): ReconcileResult {
  const values: Record<string, string | null> = {};
  const toPush: Record<string, string> = {};

  for (const key of keys) {
    const serverVal = serverPrefs[key];
    if (serverVal != null && serverVal !== "") {
      window.localStorage.setItem(key, serverVal);
      values[key] = serverVal;
      continue;
    }
    const localVal = window.localStorage.getItem(key);
    if (localVal != null && localVal !== "") {
      toPush[key] = localVal;
      values[key] = localVal;
    } else {
      values[key] = null;
    }
  }

  return { values, toPush };
}
